fn main() {
    println!("Hello, world!");
    let mut x = 5;
    let q = &mut x;
    x = 3;
    println!(*q);
}
