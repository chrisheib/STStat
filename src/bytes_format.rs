pub fn format_bytes(b: f64) -> String {
    let mut b = b;
    if b < 1024.0 {
        return format!("{b:.0} B");
    } else {
        b /= 1024.0;
    }
    if b < 1024.0 {
        if b < 10.0 {
            return format!("{b:.1} KB");
        } else {
            return format!("{b:.0} KB");
        }
    } else {
        b /= 1024.0;
    }
    if b < 1024.0 {
        if b < 10.0 {
            return format!("{b:.1} MB");
        } else {
            return format!("{b:.0} MB");
        }
    } else {
        b /= 1024.0;
    }
    if b < 1024.0 {
        if b < 10.0 {
            return format!("{b:.1} GB");
        } else {
            return format!("{b:.0} GB");
        }
    } else {
        b /= 1024.0;
    }
    if b < 1024.0 {
        if b < 10.0 {
            return format!("{b:.1} TB");
        } else {
            return format!("{b:.0} TB");
        }
    } else {
        b /= 1024.0;
    }
    if b < 1024.0 {
        if b < 10.0 {
            format!("{b:.1} PB")
        } else {
            format!("{b:.0} PB")
        }
    } else {
        b /= 1024.0;
        if b < 10.0 {
            format!("{b:.1} EB")
        } else {
            format!("{b:.0} EB")
        }
    }
}
