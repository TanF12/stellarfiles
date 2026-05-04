mod app;
mod errors;
mod file_ops;
mod math;
mod types;

rust_i18n::i18n!("locales", fallback = "en");

use anyhow::Result;
use app::state::FileApp;
use clap::Parser;
use std::collections::HashMap;
use std::path::PathBuf;
use types::{AppFlags, Mode, PortalRequest};
use zbus::{
    connection::Builder,
    interface,
    zvariant::{ObjectPath, OwnedValue, Value},
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub pick_file: bool,
    #[arg(long, value_name = "DEFAULT_NAME")]
    pub save_file: Option<String>,
}

struct FileChooserPortal {
    app_tx: async_channel::Sender<PortalRequest>,
}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooserPortal {
    #[zbus(name = "OpenFile")]
    async fn open_file(
        &self,
        _handle: ObjectPath<'_>,
        _app_id: String,
        _parent_window: String,
        _title: String,
        _options: HashMap<String, Value<'_>>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let (tx, rx) = async_channel::bounded(1);
        if self.app_tx.send(PortalRequest::OpenFile(tx)).await.is_ok() {
            let path = rx.recv().await.unwrap_or_default();
            if path.is_empty() {
                return (1, HashMap::new()); // 1 = user cancelled
            }
            let mut results = HashMap::new();
            let uris = vec![format!("file://{}", path)];
            let val = Value::from(uris);
            let owned = OwnedValue::try_from(val).unwrap();
            results.insert("uris".to_string(), owned);
            (0, results) // 0 = success
        } else {
            (2, HashMap::new()) // 2 = other error
        }
    }

    #[zbus(name = "SaveFile")]
    async fn save_file(
        &self,
        _handle: ObjectPath<'_>,
        _app_id: String,
        _parent_window: String,
        _title: String,
        options: HashMap<String, Value<'_>>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let mut default_name = String::new();
        if let Some(v) = options.get("current_name")
            && let Ok(name) = String::try_from(v.clone())
        {
            default_name = name;
        }

        let (tx, rx) = async_channel::bounded(1);
        if self
            .app_tx
            .send(PortalRequest::SaveFile(default_name, tx))
            .await
            .is_ok()
        {
            let path = rx.recv().await.unwrap_or_default();
            if path.is_empty() {
                return (1, HashMap::new()); // cancelled
            }
            let mut results = HashMap::new();
            let uris = vec![format!("file://{}", path)];
            let val = Value::from(uris);
            let owned = OwnedValue::try_from(val).unwrap();
            results.insert("uris".to_string(), owned);
            (0, results)
        } else {
            (2, HashMap::new()) // error
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let locale = sys_locale::get_locale().unwrap_or_else(|| "en_US".to_string());
    rust_i18n::set_locale(&locale);

    let (app_tx, app_rx) = async_channel::unbounded();
    let start_path = if let Some(p) = cli.path {
        p.canonicalize().unwrap_or(p)
    } else {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
    };

    let mut mode = Mode::Manager;

    if cli.pick_file {
        mode = Mode::Open;
        let (tx, rx) = async_channel::bounded(1);
        let _ = app_tx.send_blocking(PortalRequest::OpenFile(tx));
        std::thread::spawn(move || {
            if let Ok(path) = rx.recv_blocking() {
                if !path.is_empty() {
                    println!("{}", path);
                }
                std::process::exit(0);
            }
        });
    } else if let Some(name) = cli.save_file {
        mode = Mode::Save(name.clone());
        let (tx, rx) = async_channel::bounded(1);
        let _ = app_tx.send_blocking(PortalRequest::SaveFile(name, tx));
        std::thread::spawn(move || {
            if let Ok(path) = rx.recv_blocking() {
                if !path.is_empty() {
                    println!("{}", path);
                }
                std::process::exit(0);
            }
        });
    } else {
        let portal_tx = app_tx.clone();
        std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Runtime::new() {
                rt.block_on(async {
                    let portal = FileChooserPortal { app_tx: portal_tx };
                    if let Ok(builder) = Builder::session()
                        && let Ok(b_name) =
                            builder.name("org.freedesktop.impl.portal.desktop.stellarfiles")
                        && let Ok(_conn) = b_name
                            .serve_at("/org/freedesktop/portal/desktop", portal)
                            .unwrap()
                            .build()
                            .await
                    {
                        std::future::pending::<()>().await;
                    }
                });
            }
        });
    }

    let flags = AppFlags {
        mode,
        portal_rx: app_rx,
        start_path,
    };

    cosmic::app::run::<FileApp>(cosmic::app::Settings::default(), flags)?;
    Ok(())
}
