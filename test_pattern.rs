fn main() {
    let s = "hello world".to_string();
    let arr: &[&str] = &["a", "b"];
    for prefix in arr {
        s.starts_with(prefix);
    }
}
