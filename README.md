# Stellarfiles
Currently in early development. Use at your own risk.

A minimalist, fast native Linux file manager.

Written in Rust. Built for the Cosmic Desktop Environment.

## Why?
Because it is good to have a competent alternative to the native Cosmic file manager.

Although my primary motivation was that the Cosmic File currently has an issue with its file chooser that causes it to enter a loop of crashes.

## Features
* **Zero-Copy Transfers** 
* **DBus Portal Hijacking**
* **Fearless Concurrency With Tokio and Rayon**
* **Regex Batch Renaming**
* **In-Process Archiving Without Shelling Out**
* **Rich Media Caching**

## Architecture
Strict Elm architecture. 

This codebase is Linux-first. So it naturally relies heavily on FreeDesktop XDG standards, DBus session buses, and UNIX file permissions.

## Build & Install

You need the standard Rust toolchain and [just](https://github.com/casey/just).

```bash
# Build and run in debug mode
just run

# Install binary, .desktop file, and DBus service to ~/.local (no root required)
just install-local

# Install system-wide
sudo just install-system

# Run the test suite
just test
```