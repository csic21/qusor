//! Ownership model using real pinned nsh_core::ImageData new/clone; Blob/slice are models.
//! No native renderer, decoder, JS GC, allocation-free or Host ACK qualification.

use super::*;
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant},
};
fn key(i: u64) -> frame_owner_map::FrameKey {
    frame_owner_map::FrameKey {
        mount_id: format!("native-ticket-{i}"),
        generation: i + 1,
        frame_id: format!("frame-{i}"),
    }
}
fn delivered(
    caller: &frame_owner_map::FrameCaller<Arc<encoded_bytes::TrackedEncodedBytes>>,
    i: u64,
    bytes: Arc<[u8]>,
) -> (
    Arc<encoded_bytes::TrackedEncodedBytes>,
    frame_scheduler::LocalResultLease,
) {
    let mut job = caller
        .submit(key(i), move |cancel| {
            Ok(Arc::new(encoded_bytes::TrackedEncodedBytes::new(
                bytes,
                cancel.owner(),
            )?))
        })
        .unwrap();
    let end = Instant::now() + Duration::from_secs(1);
    loop {
        match Pin::new(&mut job).poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(Ok(d)) => return d.into_local_value(),
            Poll::Ready(Err(e)) => panic!("{e}"),
            Poll::Pending => {
                assert!(Instant::now() < end);
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
#[test]
fn more_than64_deliver_close_reclaim_with_an_old_encoded_guard_still_alive() {
    let baseline = encoded_bytes::snapshot().unwrap();
    let caller = frame_owner_map::FrameCaller::new().unwrap();
    let bytes: Arc<[u8]> = Arc::from([1u8, 2, 3, 4]);
    let (old, lease) = delivered(&caller, 0, bytes.clone());
    lease.record_bridge_attempt().unwrap();
    drop(lease);
    for i in 1..130 {
        let (data, lease) = delivered(&caller, i, bytes.clone());
        lease.record_bridge_attempt().unwrap();
        drop(data);
        drop(lease);
        let retained = old.owner_snapshot().unwrap();
        assert_eq!(retained.encoded_containers, 1);
        assert_eq!(retained.encoded_referenced_bytes, 4);
        assert_eq!(retained.handed_to_bridge, 1);
        let summary = encoded_bytes::snapshot().unwrap();
        assert_eq!(summary.containers, baseline.containers + 1);
        assert!(
            caller.snapshot_owners().unwrap().len() <= 3,
            "historical task map accumulated"
        );
    }
    let second = frame_owner_map::FrameCaller::new().unwrap();
    let (new, lease) = delivered(&second, 0, bytes.clone());
    assert!(!old.same_owner(&new));
    lease.record_bridge_attempt().unwrap();
    drop(lease);
    assert_eq!(old.owner_snapshot().unwrap().encoded_containers, 1);
    drop(new);
    assert_eq!(old.owner_snapshot().unwrap().encoded_containers, 1);
    let a = caller.retirement_ticket();
    let b = second.retirement_ticket();
    caller.close().unwrap();
    second.close().unwrap();
    assert!(
        a.wait_until(Instant::now() + Duration::from_secs(1))
            .unwrap()
    );
    assert!(
        b.wait_until(Instant::now() + Duration::from_secs(1))
            .unwrap()
    );
    assert_eq!(
        encoded_bytes::snapshot().unwrap().containers,
        baseline.containers + 1
    );
    drop(old);
    assert_eq!(
        encoded_bytes::snapshot().unwrap().containers,
        baseline.containers
    );
    assert_eq!(Arc::strong_count(&bytes), 1); // modeled independent cache Arc remains
}
// Use the actual pinned NSH sharing type; Blob/slice remain an explicit model.
struct SharedTracked(Arc<encoded_bytes::TrackedEncodedBytes>);
impl AsRef<[u8]> for SharedTracked {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref().as_ref()
    }
}
#[derive(Clone)]
struct Blob {
    data: ImageData,
    start: usize,
    end: usize,
}
impl Blob {
    fn image_data(&self) -> ImageData {
        if self.start == 0 && self.end == self.data.bytes().len() {
            self.data.clone()
        } else {
            ImageData::new(BlobBytes(self.clone()))
        }
    }
}
struct BlobBytes(Blob);
impl AsRef<[u8]> for BlobBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0.data.bytes()[self.0.start..self.0.end]
    }
}
#[test]
fn shared_full_slice_retainer_and_decoded_copy_have_distinct_lifetimes() {
    let baseline = encoded_bytes::snapshot().unwrap();
    let caller = frame_owner_map::FrameCaller::new().unwrap();
    let png_cache: Arc<[u8]> = Arc::from([5u8, 6, 7, 8]);
    let (tracked, lease) = delivered(&caller, 300, png_cache.clone());
    lease.record_bridge_attempt().unwrap();
    let image = ImageData::new(SharedTracked(tracked.clone()));
    let file = Blob {
        data: image.clone(),
        start: 0,
        end: 4,
    };
    let url = file.clone();
    let full_source = url.image_data();
    let slice = Blob {
        data: file.data.clone(),
        start: 1,
        end: 3,
    };
    let sliced_source = slice.image_data();
    let shared_retainer = full_source.clone();
    let decoded_copy = full_source.bytes().to_vec();
    drop(tracked);
    drop(lease);
    drop(image);
    drop(file);
    drop(url);
    drop(full_source);
    drop(slice);
    assert_eq!(
        encoded_bytes::snapshot().unwrap().containers,
        baseline.containers + 1
    );
    assert_eq!(sliced_source.bytes(), [6, 7]);
    drop(sliced_source);
    assert_eq!(
        encoded_bytes::snapshot().unwrap().containers,
        baseline.containers + 1
    ); // generic shared retainer; do NOT label it mount pin or actual L3
    drop(shared_retainer);
    assert_eq!(
        encoded_bytes::snapshot().unwrap().containers,
        baseline.containers
    );
    assert_eq!(decoded_copy, [5, 6, 7, 8]);
    assert_eq!(Arc::strong_count(&png_cache), 1);
    let ticket = caller.retirement_ticket();
    caller.close().unwrap();
    assert!(
        ticket
            .wait_until(Instant::now() + Duration::from_secs(1))
            .unwrap()
    );
}
