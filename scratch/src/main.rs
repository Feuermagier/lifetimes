#[derive(Debug)]
struct A(u32);

fn main() {
    let mut x = 42;
    let y = &mut x;
    let z = &mut x;

    // Why??
    /*
    let mut x = A(0);
    let y = &mut x;
    let z = &mut (*y);
    *y = A(2);
    *z = A(3);
    */
}
