use rehow_core::domain::Guide;

fn requires_serialize<T: serde::Serialize>() {}

fn main() {
    requires_serialize::<Guide>();
}
