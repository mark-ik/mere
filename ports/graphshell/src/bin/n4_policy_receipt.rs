use std::path::PathBuf;

use session_runtime::PersonaId;

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: n4_policy_receipt <output.html>");
    let state = std::env::temp_dir().join(format!(
        "graphshell-n4-receipt-state-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&state);

    let receipt =
        graphshell::policy_projection::run_n4_policy_scenario(&state, PersonaId::default_persona())
            .expect("N4 policy scenario");
    let html = graphshell::policy_projection::render_n4_policy_receipt(&receipt);
    std::fs::write(&output, html).expect("receipt writes");
    std::fs::remove_dir_all(state).expect("receipt state cleans up");
    println!("wrote {}", output.display());
}
