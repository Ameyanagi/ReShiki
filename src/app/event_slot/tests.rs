use super::*;
use iced::futures::{StreamExt, pin_mut};

#[test]
fn install_replaces_the_receiver_and_take_transfers_it_once() {
    let slot = Slot::new();
    assert_eq!(slot.take(), None);
    slot.install(1);
    slot.install(2);
    assert_eq!(slot.take(), Some(2));
    assert_eq!(slot.take(), None);
}

async fn forwarded<R: Receiver<Item = u8>>(receiver: R) {
    let slot = Box::leak(Box::new(Slot::new()));
    slot.install(receiver);
    let source = Source {
        slot,
        capacity: 1,
        forward: |value| Message::InspectorScroll(f32::from(value)),
    };
    let events = stream(&source);
    pin_mut!(events);
    for expected in [3., 7.] {
        assert!(matches!(
            events.next().await,
            Some(Message::InspectorScroll(value)) if value == expected
        ));
    }
    assert!(events.next().await.is_none());
    assert!(slot.take().is_none());
    let restarted = stream(&source);
    pin_mut!(restarted);
    assert!(restarted.next().await.is_none());
}

#[tokio::test]
async fn bounded_receiver_forwards_in_order_and_is_not_retaken() {
    let (sender, receiver) = mpsc::channel(2);
    sender.try_send(3).unwrap();
    sender.try_send(7).unwrap();
    drop(sender);
    forwarded(receiver).await;
}

#[tokio::test]
async fn unbounded_receiver_forwards_in_order_and_is_not_retaken() {
    let (sender, receiver) = mpsc::unbounded_channel();
    sender.send(3).unwrap();
    sender.send(7).unwrap();
    drop(sender);
    forwarded(receiver).await;
}
