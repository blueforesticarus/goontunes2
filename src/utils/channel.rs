// Maybe this should hold the sender and receiver
pub struct Mpsc<T, const S: usize>(std::marker::PhantomData<T>);

pub trait Channel {
    type Sender: postage::sink::Sink;
    type Receiver: postage::stream::Stream;

    fn new() -> (Self::Sender, Self::Receiver);
}

impl<T, const S: usize> Channel for Mpsc<T, S> {
    type Sender = postage::mpsc::Sender<T>;
    type Receiver = postage::mpsc::Receiver<T>;

    fn new() -> (Self::Sender, Self::Receiver) {
        postage::mpsc::channel(S)
    }
}
