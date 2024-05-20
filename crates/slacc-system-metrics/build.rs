fn main() {
    if std::env::var("TARGET").unwrap().contains("darwin") {
        println!("cargo:rustc-link-lib=framework=IOKit");
    }
}
