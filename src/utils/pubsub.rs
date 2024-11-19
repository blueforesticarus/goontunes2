use std::sync::LazyLock;

use kameo::{
    actor::{
        pubsub::{PubSub, Publish, Subscribe},
        ActorRef,
    },
    error::SendError,
    message::Message,
    request::{LocalTellRequest, MessageSend, TellRequest, WithoutRequestTimeout},
    Actor, Reply,
};

pub static PUBSUB: LazyLock<PSBroker> = LazyLock::new(Default::default);

#[derive(Default)]
pub struct PSBroker(tokio::sync::RwLock<type_map::concurrent::TypeMap>);

pub trait Topic {}

impl PSBroker {
    pub async fn publish<M: 'static + Clone + Send + Topic>(
        &self,
        msg: M,
    ) -> Result<(), SendError<Publish<M>>> {
        type Thing<M> = ActorRef<PubSub<M>>;
        let g = self.0.read().await;
        if let Some(ps) = g.get::<Thing<M>>() {
            ps.tell(Publish(msg)).send().await?;
        }

        Ok(())
    }

    pub async fn subscribe<M, A>(
        &self,
        actor_ref: ActorRef<A>,
    ) -> Result<(), SendError<Subscribe<A>>>
    where
        A: Actor + Message<M>,
        M: Send + 'static,
        for<'a> TellRequest<LocalTellRequest<'a, A, A::Mailbox>, A::Mailbox, M, WithoutRequestTimeout>:
            MessageSend<Ok = (), Error = SendError<M, <A::Reply as Reply>::Error>>,
    {
        type Thing<M> = ActorRef<PubSub<M>>;
        let mut g = self.0.write().await;
        let ps = match g.get::<Thing<M>>() {
            Some(ps) => ps,
            None => {
                let ps = kameo::spawn(PubSub::<M>::new());
                g.insert(ps);
                g.get::<Thing<M>>().unwrap()
            }
        };

        ps.tell(Subscribe(actor_ref)).send().await
    }
}

#[cfg(test)]
mod tests {
    use std::slice::range;
    use std::time::Duration;

    use crate::utils::pubsub::PUBSUB;

    use futures::SinkExt;
    use futures::{channel::mpsc, StreamExt};
    use kameo::{
        error::SendError,
        mailbox::{
            bounded::{BoundedMailbox, BoundedMailboxReceiver},
            unbounded::UnboundedMailbox,
        },
        message::{Context, Message},
        messages,
        request::{
            BlockingMessageSend, MessageSend, MessageSendSync, TryBlockingMessageSend,
            TryMessageSend, TryMessageSendSync,
        },
        spawn, Actor,
    };

    #[tokio::test]
    async fn test_broker() {
        struct MyActor(mpsc::Sender<()>);

        impl Actor for MyActor {
            type Mailbox = BoundedMailbox<Self>;
        }

        #[messages]
        impl MyActor {
            #[message(derive(Clone))]
            async fn say_hi(&mut self) {
                println!("hello");
                self.0.send(()).await;
            }
        }

        let mut chan = mpsc::channel(1);
        let actor_ref = kameo::spawn(MyActor(chan.0));
        PUBSUB.subscribe(actor_ref).await;
        tokio::time::timeout(Duration::from_millis(100), chan.1.next())
            .await
            .unwrap();
    }
}

// #[derive(Clone)]
// struct Topic;
// impl Topic {
//     fn broker() -> MutexGuard<'static, PubSub<Self>> {
//         static PUBSUB: LazyLock<Mutex<PubSub<Topic>>> = LazyLock::new(Default::default);
//         PUBSUB.lock()
//     }

//     fn publish(self) {
//         Self::broker().publish(self);
//     }
//     fn subscribe<A>(actor_ref: ActorRef<A>)
//     where
//         A: kameo::Actor + Message<Self>,
//         for<'a> TellRequest<LocalTellRequest<'a, A, A::Mailbox>, A::Mailbox, Self, WithoutRequestTimeout>:
//             MessageSend<Ok = (), Error = SendError<Self, <A::Reply as Reply>::Error>>,
//     {
//         static PUBSUB: LazyLock<Mutex<PubSub<Topic>>> = LazyLock::new(Default::default);
//         Self::broker().subscribe(actor_ref);
//     }
// }
