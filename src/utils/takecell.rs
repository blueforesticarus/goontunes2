use parking_lot::Mutex;

pub struct TakeCell<T>(Mutex<Option<T>>);

impl<T> From<T> for TakeCell<T> {
    fn from(value: T) -> Self {
        TakeCell(Mutex::new(Some(value)))
    }
}

#[derive(Debug, Clone)]
pub struct AlreadyTaken;
impl<T> TakeCell<T> {
    pub fn take(&self) -> Result<T, AlreadyTaken> {
        let mut guard = self.0.lock();
        guard.take().ok_or(AlreadyTaken)
    }
}
