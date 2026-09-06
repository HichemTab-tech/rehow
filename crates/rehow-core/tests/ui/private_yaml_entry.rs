use rehow_core::domain::{UnknownMetadataEntry, UnknownYamlDocumentBuilder, UnknownYamlEvent};

fn main() {
    let mut builder = UnknownYamlDocumentBuilder::new();
    builder.push_event(UnknownYamlEvent::BeginMapping).unwrap();
    builder.push_event(UnknownYamlEvent::String("key".into())).unwrap();
    builder.push_event(UnknownYamlEvent::Null).unwrap();
    builder.push_event(UnknownYamlEvent::EndMapping).unwrap();
    let document = builder.finish_document().unwrap();
    let value = document.entries()[0].value().clone();
    let _ = UnknownMetadataEntry {
        key: "key".into(),
        value,
    };
}
