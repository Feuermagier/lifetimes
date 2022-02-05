use lifetimes_backend::check;

#[test]
fn multi_borrow_of_immutable() {
    assert!(check(
        r#"
        fn main() {
            let x = 42;
            let y = &x;
            let z = &x;
        }"#
        .to_string(),
    )
    .is_ok());
}

#[test]
fn multi_borrow_of_mutable() {
    assert!(check(
        r#"
        fn main() {
            let mut x = 42;
            let y = &x;
            let z = &x;
        }"#
        .to_string()
    )
    .is_ok());
}

#[test]
fn multi_mut_borrow_unused() {
    assert!(check(
        r#"
        fn main() {
            let mut x = 42;
            let y = &mut x;
            let z = &mut x;
        }"#
        .to_string()
    )
    .is_ok());
}

#[test]
fn multi_mut_borrow_used() {
    assert!(check(
        r#"
        fn main() {
            let mut x = 42;
            let y = &mut x;
            let z = &mut x;
            y;
        }"#
        .to_string()
    )
    .is_err());
}

#[test]
fn move_value() {
    assert!(check(
        r#"
        fn main() {
            let x = 42;
            let y = x;
        }"#
        .to_string()
    )
    .is_ok());
}

#[test]
fn use_moved_value() {
    assert!(check(
        r#"
        fn main() {
            let x = 42;
            let y = x;
            x;
        }"#
        .to_string()
    )
    .is_err());
}

#[test]
fn borrow_moved_value() {
    assert!(check(
        r#"
        fn main() {
            let x = 42;
            let y = x;
            let z = &x;
        }"#
        .to_string()
    )
    .is_err());
}

#[test]
fn borrow_moved_value_mut() {
    assert!(check(
        r#"
        fn main() {
            let mut x = 42;
            let y = x;
            let z = &mut x;
        }"#
        .to_string()
    )
    .is_err());
}
