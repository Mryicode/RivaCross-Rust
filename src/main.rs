#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod events;
mod image_input;
mod instance;
mod model;
mod output;
mod render;
mod rtss;
mod rtss_tags;
mod storage;
mod worker;

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|a| a == "--query-zoom") {
        println!("{}", rtss::query_global_zoom()?);
        return Ok(());
    }
    if std::env::args().any(|a| a == "--inspect") {
        println!(
            "{}",
            rtss::Mapping::open()?.own_text()?.map_or_else(
                || "released",
                |b| if b.is_empty() { "hidden" } else { "visible" }
            )
        );
        return Ok(());
    }
    let Some(instance) = instance::Instance::acquire()? else {
        return Ok(());
    };
    if std::env::args().any(|a| a == "--probe") {
        return probe();
    }
    let store = {
        #[cfg(feature = "ui-capture")]
        if let Some(path) = std::env::var_os("RIVACROSS_TEST_DATA_DIR") {
            storage::Store::at(path.into())?
        } else {
            storage::Store::open()?
        }
        #[cfg(not(feature = "ui-capture"))]
        storage::Store::open()?
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([720., 800.])
            .with_min_inner_size([600., 650.]),
        ..Default::default()
    };
    eframe::run_native(
        "RivaCross",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, store, instance)))),
    )
    .map_err(|e| anyhow::anyhow!("窗口初始化失败：{e}"))
}
fn probe() -> anyhow::Result<()> {
    let store = storage::Store::at(std::env::current_dir()?.join("target/probe"))?;
    let mut m = rtss::Mapping::open()?;
    for c in [
        model::Crosshair::default(),
        model::Crosshair {
            kind: model::Kind::Text,
            ..Default::default()
        },
        model::Crosshair {
            x: 40,
            y: -20,
            ..Default::default()
        },
    ] {
        let (t, _) = crate::output::tags(&c, &store, effective_zoom(&c))?;
        let bytes = rtss::encode(&t)?;
        let start = std::time::Instant::now();
        while !m.write(&bytes, false)? {
            anyhow::ensure!(
                start.elapsed() < std::time::Duration::from_secs(3),
                "探针等待锁超时"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        println!("Submitted: {t}");
        std::thread::sleep(std::time::Duration::from_secs(3));
    }
    m.release()?;
    println!("Owned slot released");
    Ok(())
}

fn effective_zoom(c: &crate::model::Crosshair) -> u32 {
    if c.rtss_zoom == 0 {
        crate::rtss::global_zoom()
    } else {
        c.rtss_zoom
    }
}
