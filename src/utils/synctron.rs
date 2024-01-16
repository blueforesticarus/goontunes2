use postage::sink::Sink;
use tokio::sync::RwLockReadGuard;

/// This type has a few jobs
/// 1. can be opened and closed without blocking by multible writers
/// 2. can be waited on by a single reader, woken when
///  A) all writers are done
///  B) something has been written
///
/// In a way this is an inverted RwLock
/// This is meant to simplify things. Instead of pipes with data and sync, this just syncs. Data remains in db.
#[derive(Debug)]
pub struct Synctron {
    tx: parking_lot::Mutex<postage::mpsc::Sender<()>>,
    rx: tokio::sync::RwLock<postage::mpsc::Receiver<()>>,
    count: std::sync::atomic::AtomicI32,
    lock: tokio::sync::RwLock<()>,
}

impl Default for Synctron {
    fn default() -> Self {
        let (tx, rx) = postage::mpsc::channel(1);
        Self {
            tx: tx.into(),
            rx: rx.into(),
            lock: Default::default(),
            count: Default::default(),
        }
    }
}

impl Synctron {
    pub fn dirty(&self) {
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        //signal waiting
        let _ = self.tx.lock().try_send(());
    }

    pub fn open(&self) -> RwLockReadGuard<'_, ()> {
        self.lock.blocking_read()
    }

    pub async fn wait(&self) {
        use postage::stream::Stream;
        let mut rx = self.rx.try_write().unwrap(); //XXX TODO only one thing should wait on events

        // Wait for there to be some new data
        rx.recv().await;

        // Wait for all data writers to finish producing data
        let guard = self.lock.write().await;

        // clear the tx channel
        let _ = rx.try_recv();

        drop(guard); // lock immediately released

        // Done, return to caller to handle the "event"
        // ie. there is new data and writers have finished
    }
}

/// TODO
/// Idea here is that alot of code is ideally organized in terms of a pipe,
/// with packets of data comming in, and those packets can be handled in bulk or in pieces
/// often you need to respond to the end of a bulk packet. or the end of all concurrent bulk packets
///
/// However, all this data being in the pipe makes it hard to reason about program state. And especially to persist state.
/// My idea is that perhaps what I want is a datatype that *acts like* a pipe with data, while the data itself lives elsewhere.
///
/// Model:
///     writers open blocks
///     blocks are populated with messages asyncronously untill they are closed/dropped
///     readers subscribe to events
///         - on block start / end
///         - on message
///         - on busy / free
///     a pipe is busy if any block is open, free otherwise
///
///     a pipe has another busy / free state based on whether reading is waiting on data TODO
///         this allows backpressure.
struct LitePipe;

impl LitePipe {}
