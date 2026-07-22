use ashpd::desktop::settings::{ColorScheme, Settings};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> ashpd::Result<()> {
    let proxy = Settings::new().await?;

    let color_scheme = proxy
        .read::<ColorScheme>("org.freedesktop.appearance", "color-scheme")
        .await?;
    println!("{:#?}", color_scheme);

    while let Some(setting) = proxy.receive_setting_changed().await?.next().await {
        println!("{setting:#?}");
    }

    Ok(())
}
