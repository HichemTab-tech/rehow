use rehow_core::domain::Guide;

fn mutate(value: &mut Guide) {
    value.title = "changed".to_owned();
}

fn main() {}
