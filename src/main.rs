use std::path::{Path, PathBuf};

use anyhow::Context;
use ashpd::{
    desktop::settings::{ColorScheme, Settings},
    zbus,
};
use futures_util::StreamExt;
use sysinfo::{Signal, System};
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let hx_config_path: PathBuf = args
        .next()
        .context("missing config path argument")?
        .parse()
        .context("invalid config path argument")?;
    let dbus_socket_addr = args.next().map(Ok).unwrap_or_else(|| {
        std::env::var("DBUS_SESSION_BUS_ADDRESS").context(
            "no dbus address argument was passed but missing DBUS_SESSION_BUS_ADDRESS env var",
        )
    })?;

    let mut config_path = hx_config_path.clone();
    config_path.pop();
    config_path = config_path.join("auto_themes.toml");

    let connection = zbus::connection::Builder::address(dbus_socket_addr.as_str())
        .context("failed to parse dbus socket address")?
        .build()
        .await
        .context("failed to connect to dbus socket")?;
    let proxy = Settings::with_connection(connection).await?;
    let color_scheme = proxy
        .read::<ColorScheme>("org.freedesktop.appearance", "color-scheme")
        .await
        .context("failed to read initial color scheme value")?;
    set_helix_theme(&hx_config_path, &config_path, color_scheme).await?;
    notify_helix();

    while let Some(color_scheme) = proxy
        .receive_setting_changed_with_args::<ColorScheme>(
            "org.freedesktop.appearance",
            "color-scheme",
        )
        .await?
        .next()
        .await
    {
        let color_scheme = color_scheme.context("failed to parse as color scheme")?;
        println!("received new color scheme setting {:?}", color_scheme);
        set_helix_theme(&hx_config_path, &config_path, color_scheme).await?;
        notify_helix();
    }

    Ok(())
}

async fn set_helix_theme(
    hx_config_path: &Path,
    config_path: &Path,
    scheme: ColorScheme,
) -> anyhow::Result<()> {
    println!("reading auto themes config from {config_path:?}");
    let config = tokio::fs::read_to_string(config_path)
        .await
        .context("failed to read auto themes config")?;
    let config =
        toml_edit::Document::parse(config).context("failed to parse auto themes config")?;
    let key = match scheme {
        ColorScheme::NoPreference => "no-preference",
        ColorScheme::PreferLight => "prefer-light",
        ColorScheme::PreferDark => "prefer-dark",
    };
    let value = config
        .get("themes")
        .context("missing themes config")?
        .get(key)
        .context("missing scheme key")?
        .as_str()
        .context("theme must be a string")?;

    let buf = tokio::fs::read_to_string(hx_config_path)
        .await
        .context("failed to read config file")?;

    let mut hx_config: toml_edit::DocumentMut =
        buf.parse().context("failed to parse config file as toml")?;
    hx_config["theme"] = toml_edit::value(value);

    println!("writing to helix config at {hx_config_path:?}");
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(hx_config_path)
        .await
        .context("failed to open config file for writing")?;
    file.write_all(hx_config.to_string().as_bytes())
        .await
        .context("failed to update config file")?;

    println!("updated config file at {config_path:?}, new theme: {value}");

    Ok(())
}

fn notify_helix() {
    let system = System::new_all();

    for process in system.processes_by_exact_name("helix".as_ref()) {
        if let Some(true) = process.kill_with(Signal::User1) {
            println!("sent USER1 signal to {}", process.pid());
        } else {
            println!("failed to send USER1 signal to {}", process.pid());
        }
    }
}
