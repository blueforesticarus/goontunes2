use std::any::type_name;

fn generic<T>() -> Option<T> {
    println!("{}", type_name::<T>());
    panic!()
}

fn concrete() -> Option<usize> {
    generic()?;

    Some(0)
}

fn main() {
    concrete().unwrap();
}
