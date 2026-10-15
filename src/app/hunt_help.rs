use crate::model::HuntWeapon;
use eframe::egui::{self, RichText};

/// Edited for reading in the UI from assets/文案.md; credits remain with each asset.
pub(super) fn show(ui: &mut egui::Ui, weapon: HuntWeapon) {
    ui.add_space(8.);
    egui::Frame::group(ui.style()).inner_margin(10.).show(ui, |ui| {
        egui::CollapsingHeader::new(format!("{} · 使用说明", weapon.label()))
            .id_salt(("hunt-help", weapon.label()))
            .default_open(true)
            .show(ui, |ui| {
                let rows: &[(&str, &str)] = match weapon {
                    HuntWeapon::Crossbow => {
                        ui.label(RichText::new("适用弹种：钢箭 / 霰弹箭").strong());
                        &[
                            ("水平 · 黄色最外端", "霰弹箭：跑步目标提前量"),
                            ("水平 · 白色中段", "霰弹箭：走路目标提前量；钢箭：跑步目标提前量"),
                            ("水平 · 最内侧", "钢箭：走路目标提前量"),
                            ("下方 · 胸靶横线", "白色对应钢箭，黄色对应霰弹箭"),
                        ]
                    }
                    HuntWeapon::Bow => {
                        ui.label(RichText::new("水平标线：由内向外阅读").strong());
                        &[
                            ("① 最内侧", "腰射致死胸围"),
                            ("② 中间", "行走目标提前量"),
                            ("③ 最外侧", "跑步目标提前量"),
                            ("垂直标线", "卡胸宽"),
                        ]
                    }
                    HuntWeapon::SniperCrossbow => {
                        ui.label(RichText::new("适用屏幕比例：16:10").strong());
                        ui.small("当前为可逐线编辑的矢量重绘版；16:9 尚未校准。");
                        &[
                            ("上方 · 胸靶", "钢弩箭"),
                            ("下方 · 胸靶", "霰弹弩"),
                        ]
                    }
                };
                ui.add_space(6.);
                egui::Grid::new(("hunt-help-rows", weapon.label()))
                    .num_columns(2)
                    .spacing([18., 6.])
                    .max_col_width((ui.available_width() - 160.).max(120.))
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(RichText::new("标线位置").strong());
                        ui.label(RichText::new("用途").strong());
                        ui.end_row();
                        for &(position, meaning) in rows {
                            ui.label(position);
                            ui.add(egui::Label::new(meaning).wrap());
                            ui.end_row();
                        }
                    });
                if weapon == HuntWeapon::Bow {
                    ui.add_space(6.);
                    ui.add(egui::Label::new("作者备注：弓箭瞄准移动目标仍较困难，实用性可能有限；因靶场加入移动靶，本次更新了提前量。").wrap());
                }
                ui.separator();
                ui.small("素材来源与致谢");
                match weapon {
                    HuntWeapon::Crossbow | HuntWeapon::Bow => {
                        ui.label("感谢 bilibili @Zuko819470293 开源。");
                    }
                    HuntWeapon::SniperCrossbow => {
                        ui.label("Steam · Crosshair 创意工坊");
                        ui.add(egui::Label::new("Hunt:showdown_crossbow_sniper_steel&slug_16:10").wrap());
                        ui.hyperlink_to("查看创意工坊原作 ↗", "https://steamcommunity.com/sharedfiles/filedetails/?id=3316676875");
                    }
                }
            });
    });
}
