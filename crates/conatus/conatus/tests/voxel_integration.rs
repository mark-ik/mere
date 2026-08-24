use conatus::{
    BodyDesc, BodyWorld, ColliderDesc, ColliderId, ColliderShape, VoxelCellEdit, VoxelChunk,
};

#[test]
fn accepted_chunk_patch_updates_the_voxel_collider() {
    let mut chunk = VoxelChunk::new([4, 4, 4], 0u8).unwrap();
    chunk
        .apply_edits(
            0,
            1,
            [VoxelCellEdit {
                cell: [0, 0, 0],
                value: 1,
            }],
        )
        .unwrap();

    let mut world = BodyWorld::new([0.0; 3]);
    let body = world
        .spawn(
            BodyDesc::fixed().with_collider(ColliderDesc::new(ColliderShape::VoxelGrid {
                cell_size: [1.0; 3],
                occupied: chunk.occupied_cells(|material| *material != 0),
            })),
        )
        .unwrap();

    let patch = chunk
        .apply_edits(
            1,
            3,
            [
                VoxelCellEdit {
                    cell: [0, 0, 0],
                    value: 2,
                },
                VoxelCellEdit {
                    cell: [1, 0, 0],
                    value: 3,
                },
            ],
        )
        .unwrap();
    let occupancy_edits = patch.occupancy_edits(|material| *material != 0).unwrap();
    assert_eq!(occupancy_edits.len(), 1, "material-only edits stay out");

    let summary = world
        .edit_voxels(ColliderId::new(body, 0), occupancy_edits)
        .unwrap();
    assert_eq!(summary.changed, 1);
    assert!(summary.revision > summary.previous_revision);
}
