//! Independent consuming-crate proof: only the public project-local API is used.
use transform_hierarchy_qualification::{Edit, Error, NodeId, Scene, Translation};

#[test]
fn player_weapon_camera_consumer() -> Result<(), Error> {
    let mut scene = Scene::new();
    let player = NodeId::new(0, 1);
    let weapon = NodeId::new(1, 1);
    let camera = NodeId::new(2, 1);

    scene.transact(&[
        Edit::Spawn {
            id: player,
            parent: None,
            local: Translation::new(10, 2, 1),
        },
        Edit::Spawn {
            id: weapon,
            parent: Some(player),
            local: Translation::new(2, 0, 0),
        },
        Edit::Spawn {
            id: camera,
            parent: Some(player),
            local: Translation::new(0, 3, 0),
        },
    ])?;
    assert_eq!(scene.world(weapon), Some(Translation::new(12, 2, 1)));

    scene.transact(&[Edit::Move {
        id: player,
        local: Translation::new(20, 2, 1),
    }])?;
    assert_eq!(scene.world(weapon), Some(Translation::new(22, 2, 1)));

    scene.transact(&[Edit::Reparent {
        id: weapon,
        parent: Some(camera),
    }])?;
    assert_eq!(scene.world(weapon), Some(Translation::new(22, 5, 1)));
    assert_eq!(scene.world(NodeId::new(1, 2)), None);
    Ok(())
}

#[test]
fn rejected_edit_preserves_visible_result() -> Result<(), Error> {
    let mut scene = Scene::new();
    let root = NodeId::new(0, 1);
    scene.transact(&[Edit::Spawn {
        id: root,
        parent: None,
        local: Translation::new(4, 5, 6),
    }])?;
    let before = scene.world(root);
    assert_eq!(
        scene.transact(&[Edit::Reparent {
            id: root,
            parent: Some(root),
        }]),
        Err(Error::Cycle)
    );
    assert_eq!(scene.world(root), before);
    Ok(())
}
