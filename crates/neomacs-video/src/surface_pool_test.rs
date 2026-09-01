use super::surface_pool::{BoundedSurfacePool, SurfacePoolAcquire};

#[test]
fn a_checked_out_surface_backpressures_at_capacity_and_reuses_after_retirement() {
    let pool = BoundedSurfacePool::new(1);
    let lease = match pool.acquire((1920, 1080)) {
        SurfacePoolAcquire::Allocate(reservation) => reservation.fulfill(7),
        SurfacePoolAcquire::Reused(_) | SurfacePoolAcquire::Backpressured => {
            panic!("the first acquire must reserve a surface")
        }
    };

    assert!(matches!(
        pool.acquire((1920, 1080)),
        SurfacePoolAcquire::Backpressured
    ));
    drop(lease);

    let reused = match pool.acquire((1920, 1080)) {
        SurfacePoolAcquire::Reused(lease) => lease,
        SurfacePoolAcquire::Allocate(_) | SurfacePoolAcquire::Backpressured => {
            panic!("the retired surface must be reused")
        }
    };
    assert_eq!(*reused.value(), 7);
}

#[test]
fn idle_surfaces_with_stale_geometry_are_replaced_within_the_same_bound() {
    let pool = BoundedSurfacePool::new(1);
    let old = match pool.acquire((640, 480)) {
        SurfacePoolAcquire::Allocate(reservation) => reservation.fulfill(1),
        SurfacePoolAcquire::Reused(_) | SurfacePoolAcquire::Backpressured => unreachable!(),
    };
    drop(old);

    assert!(matches!(
        pool.acquire((1280, 720)),
        SurfacePoolAcquire::Allocate(_)
    ));
}

#[test]
fn rotating_surface_identities_are_retained_until_capacity_is_needed() {
    let pool = BoundedSurfacePool::new(3);
    let first = match pool.acquire("decoder-slot-a") {
        SurfacePoolAcquire::Allocate(reservation) => reservation.fulfill(11),
        SurfacePoolAcquire::Reused(_) | SurfacePoolAcquire::Backpressured => unreachable!(),
    };
    drop(first);
    let second = match pool.acquire("decoder-slot-b") {
        SurfacePoolAcquire::Allocate(reservation) => reservation.fulfill(22),
        SurfacePoolAcquire::Reused(_) | SurfacePoolAcquire::Backpressured => unreachable!(),
    };
    drop(second);

    let reused = match pool.acquire("decoder-slot-a") {
        SurfacePoolAcquire::Reused(lease) => lease,
        SurfacePoolAcquire::Allocate(_) | SurfacePoolAcquire::Backpressured => {
            panic!("a decoder-pool identity must survive unrelated rotations")
        }
    };
    assert_eq!(*reused.value(), 11);
}
