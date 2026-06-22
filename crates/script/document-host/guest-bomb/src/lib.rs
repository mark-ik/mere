//! A deliberately-misbehaving `document-core` guest (P2.2 cancellation/quota
//! probe). It satisfies the world's exports but, on command, refuses to return:
//! `loop` spins forever (the host must trap it via epoch interruption), `grow`
//! allocates without bound (the host must deny it via StoreLimits). Any other
//! event returns an empty batch. Not shipped — it exists only to prove the host
//! contains a hostile component.

wit_bindgen::generate!({
    path: "../wit",
    world: "document-core",
});

struct Bomb;

impl Guest for Bomb {
    fn activate() {}

    fn deactivate() {}

    fn handle_event(ev: Event) -> Result<ApplyBatch, TurnError> {
        match ev.kind.as_str() {
            // Never returns: the host's epoch deadline must trap this.
            "loop" => {
                #[allow(clippy::empty_loop)]
                loop {
                    core::hint::spin_loop();
                }
            }
            // Allocates until a memory.grow is denied by StoreLimits → alloc
            // failure → abort/trap.
            "grow" => {
                let mut sink: Vec<Vec<u8>> = Vec::new();
                loop {
                    sink.push(vec![0u8; 8 * 1024 * 1024]);
                    core::hint::black_box(sink.len());
                }
            }
            _ => Ok(ApplyBatch { expected_revision: 0, changes: Vec::new() }),
        }
    }
}

export!(Bomb);
