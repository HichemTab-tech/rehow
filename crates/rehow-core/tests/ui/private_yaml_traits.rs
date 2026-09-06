use rehow_core::domain::UnknownMetadataDocument;

fn needs_equality<T: Eq>() {}

fn main() {
    needs_equality::<UnknownMetadataDocument>();
}
