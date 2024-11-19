struct A {
    a: bool,
}

impl A {
    async fn foo(&mut self) {
        self.a = true;
    }
    async fn bar(&mut self) {
        dbg!(self.a);
    }
}

#[tokio::main]
async fn main() {
    let mut a = A { a: false };

    let f1 = a.foo();
    let f2 = a.bar();

    f2.await;
    f1.await;
}
