use eframe::epaint::Color32;

pub fn auto_color(index: i32) -> Color32 {
    // let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    // let h = index as f32 * golden_ratio;
    // Hsva::new(h, 0.85, 0.5, 1.0).into() // TODO(emilk): OkLab or some other perspective color space

    // pregenerated from above formula
    match index {
        0 => Color32::from_rgba_premultiplied(188, 77, 77, 255),
        1 => Color32::from_rgba_premultiplied(77, 123, 188, 255),
        2 => Color32::from_rgba_premultiplied(154, 188, 77, 255),
        3 => Color32::from_rgba_premultiplied(188, 77, 178, 255),
        4 => Color32::from_rgba_premultiplied(77, 188, 175, 255),
        5 => Color32::from_rgba_premultiplied(188, 150, 77, 255),
        6 => Color32::from_rgba_premultiplied(118, 77, 188, 255),
        7 => Color32::from_rgba_premultiplied(86, 188, 77, 255),
        8 => Color32::from_rgba_premultiplied(188, 77, 128, 255),
        9 => Color32::from_rgba_premultiplied(77, 158, 188, 255),
        _ => todo!("auto_color({index}) not implemented yet, please report!"),
    }
}
