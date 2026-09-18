use rehow_core::domain::Category;

fn mutate(value: &mut Category) {
    value.set_name("changed".to_owned());
}

fn main() {}
