use rehow_core::domain::WorkspaceManifest;

fn requires_serialize<T: serde::Serialize>() {}

fn main() {
    requires_serialize::<WorkspaceManifest>();
}
