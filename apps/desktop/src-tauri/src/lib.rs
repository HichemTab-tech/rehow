pub const APPLICATION_NAME: &str = "Rehow";

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Rehow");
}

#[cfg(test)]
mod tests {
    use super::APPLICATION_NAME;

    #[test]
    fn application_name_is_stable() {
        assert_eq!(APPLICATION_NAME, "Rehow");
    }
}
