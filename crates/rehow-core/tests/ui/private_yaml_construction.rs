use rehow_core::domain::{UnknownYamlDocumentBuilder, UnknownYamlEvent};

fn main() {
    let event = UnknownYamlEvent::Null;
    let _ = event.clone();
    let mut builder = UnknownYamlDocumentBuilder::new();
    builder.set_limits(1, 1, 1);
    let _ = builder.partial();
}
