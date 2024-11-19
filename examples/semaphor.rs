use tokio::sync::Semaphore;

#[tokio::main]
async fn main() {
    let s = Semaphore::new(10);

    dbg!(s.available_permits());
    let g = s.acquire().await;
    dbg!(s.available_permits());

    dbg!(s.forget_permits(100));

    dbg!(s.available_permits());
    drop(g);

    dbg!(s.available_permits());
}
