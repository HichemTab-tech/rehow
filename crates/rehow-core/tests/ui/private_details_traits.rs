fn needs_clone<T: Clone>() {}
fn needs_serialize<T: serde::Serialize>() {}

fn main() {
    needs_clone::<rehow_core::protocol::CategoryNotFoundDetails>();
    needs_serialize::<rehow_core::protocol::CategoryNotFoundDetails>();
    let _: rehow_core::protocol::CategoryNotFoundDetails =
        serde_json::from_str(r#"{"categoryId":"0199a1d7-7b0d-7d8e-a38d-f936c5735e10"}"#)
            .unwrap();
}
