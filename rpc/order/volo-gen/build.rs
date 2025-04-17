use volo_build::plugin::SerdePlugin;

fn main() {
    volo_build::ConfigBuilder::default()
        .plugin(SerdePlugin)
        .write()
        .unwrap();
}
