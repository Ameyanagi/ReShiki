use super::*;

#[test]
fn supplied_conformers_keep_their_coordinates_and_dimension() -> anyhow::Result<()> {
    let source = super::super::read_smiles("CO>O>N |(1,2,3;4,5,6;7,8,9;10,11,12),(99,99,99)|")?;
    let layout = source.layout()?;
    assert_eq!(layout.requests().count(), 0);
    let drawing = layout.finish(Vec::new())?;
    let parts = drawing.participants().collect::<Vec<_>>();
    assert!(parts.iter().all(|(_, file)| file.is_3d));
    assert_eq!(
        parts
            .iter()
            .map(|(m, _)| m
                .positions
                .iter()
                .map(|p| (p.x, p.y, p.z))
                .collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec![(1., 2., 3.), (4., 5., 6.)],
            vec![(7., 8., 9.)],
            vec![(10., 11., 12.)]
        ]
    );
    Ok(())
}

#[test]
fn incomplete_or_invalid_layouts_never_finish_a_reaction() -> anyhow::Result<()> {
    let source =
        || -> anyhow::Result<Layout> { Ok(super::super::read_smiles("CC>O>N")?.layout()?) };
    for positions in [
        Vec::new(),
        vec![vec![Point3::default(); 2]],
        vec![vec![Point3::default(); 2]; 4],
        vec![
            vec![Point3::default(); 2],
            Vec::new(),
            vec![Point3::default()],
        ],
        vec![
            vec![
                Point3 {
                    x: f64::NAN,
                    y: 0.,
                    z: 0.
                };
                2
            ],
            vec![Point3::default()],
            vec![Point3::default()],
        ],
    ] {
        assert!(source()?.finish(positions).is_err());
    }
    let valid = source()?;
    assert_eq!(valid.requests().count(), 3);
    assert!(
        valid
            .finish(vec![
                vec![
                    Point3::default(),
                    Point3 {
                        x: 1.5,
                        y: 0.,
                        z: 0.
                    }
                ],
                vec![Point3::default()],
                vec![Point3::default()]
            ])
            .is_ok()
    );
    Ok(())
}
