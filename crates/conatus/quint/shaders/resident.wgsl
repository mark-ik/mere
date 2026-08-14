// The resident field lane's kernels: repulsion, springs, integration.
//
// The compiled artifact of `quint-shaders`, whose Rust source is the
// intended source of truth (see that crate's README for why WGSL is
// what ships today). The three entry points and their bindings match
// that crate's signatures exactly, so the swap is a shader-module
// change and nothing else.
//
// Positions and velocities are padded 3D `vec4f` throughout, per the
// spatial compute plan: xyz meaningful, w spare, and a 2D canvas is a
// constrained case of the same layout rather than a second format.

struct Params {
    n: u32,
    dt: f32,
    damping: f32,
    repulsion: f32,
    min_distance: f32,
    spring_k: f32,
    rest_length: f32,
    centering: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> positions: array<vec4f>;
@group(0) @binding(2) var<storage, read_write> velocities: array<vec4f>;
@group(0) @binding(3) var<storage, read_write> forces: array<vec4f>;
@group(0) @binding(4) var<storage, read> adjacency_offsets: array<u32>;
@group(0) @binding(5) var<storage, read> adjacency_targets: array<u32>;
@group(0) @binding(6) var<storage, read_write> settle: atomic<u32>;

const WORKGROUP: u32 = 256u;
var<workgroup> tile: array<vec4f, 256>;

// Softened inverse-square repulsion over every pair, tiled through
// workgroup memory.
//
// The same law as `quint::forces::repulsion` and its CPU anchor
// `repulsion_reference`: the self term contributes nothing because its
// displacement is zero, so no diagonal mask is needed. Threads past
// `n` still load and barrier, because a workgroup that diverges at a
// barrier is undefined behaviour.
@compute @workgroup_size(256)
fn repulse(
    @builtin(global_invocation_id) gid: vec3u,
    @builtin(local_invocation_id) lid: vec3u,
) {
    let i = gid.x;
    let mine = positions[min(i, params.n - 1u)].xyz;
    let softening = params.min_distance * params.min_distance;
    var force = vec3f(0.0);

    let tiles = (params.n + WORKGROUP - 1u) / WORKGROUP;
    for (var t = 0u; t < tiles; t = t + 1u) {
        let j = t * WORKGROUP + lid.x;
        tile[lid.x] = positions[min(j, params.n - 1u)];
        workgroupBarrier();
        let count = min(WORKGROUP, params.n - t * WORKGROUP);
        for (var k = 0u; k < count; k = k + 1u) {
            let d = mine - tile[k].xyz;
            let d2 = dot(d, d) + softening;
            force = force + d * (params.repulsion / (d2 * sqrt(d2)));
        }
        workgroupBarrier();
    }

    if (i < params.n) {
        forces[i] = vec4f(force, 0.0);
    }
}

// Spring forces gathered along CSR adjacency: every write owned by one
// invocation, so no float atomics are needed (WGSL has none).
@compute @workgroup_size(256)
fn springs(@builtin(global_invocation_id) gid: vec3u) {
    let i = gid.x;
    if (i >= params.n) {
        return;
    }
    let p = positions[i].xyz;
    var f = forces[i].xyz;
    for (var e = adjacency_offsets[i]; e < adjacency_offsets[i + 1u]; e = e + 1u) {
        let q = positions[adjacency_targets[e]].xyz;
        let d = q - p;
        let length_ = max(length(d), 1e-4);
        f = f + d * (params.spring_k * (length_ - params.rest_length) / length_);
    }
    forces[i] = vec4f(f, 0.0);
}

// Damped symplectic Euler, a weak centering, and the settle reduction.
//
// Speed is bitcast to u32 for the atomic maximum: the bit pattern of a
// non-negative float orders the same way the float does, so one
// atomicMax over the workgroup grid is the whole convergence probe.
@compute @workgroup_size(256)
fn integrate(@builtin(global_invocation_id) gid: vec3u) {
    let i = gid.x;
    if (i >= params.n) {
        return;
    }
    let p = positions[i].xyz;
    var v = velocities[i].xyz;
    let f = forces[i].xyz - p * params.centering;

    v = (v + f * params.dt) * params.damping;
    let np = p + v * params.dt;

    positions[i] = vec4f(np, positions[i].w);
    velocities[i] = vec4f(v, 0.0);
    atomicMax(&settle, bitcast<u32>(length(v)));
}
