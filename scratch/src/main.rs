#[derive(Debug)]
struct A(u32);

fn main() {
    let mut a = 1;
    let mut b = 2;

    let x = if true {
        &mut a
    } else {
        &b
    };

    // Why??
    /*
    let mut x = A(0);
    let y = &mut x;
    let z = &mut (*y);
    *y = A(2);
    *z = A(3);
    */
}
