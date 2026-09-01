//! Target-aware production allocator selection for Neomacs.

cfg_select! {
    target_os = "linux" => {
        pub use tikv_jemallocator::Jemalloc as PlatformAllocator;

        pub const PLATFORM_ALLOCATOR: PlatformAllocator = PlatformAllocator;
    }
    _ => {
        pub use mimalloc::MiMalloc as PlatformAllocator;

        pub const PLATFORM_ALLOCATOR: PlatformAllocator = PlatformAllocator;
    }
}
