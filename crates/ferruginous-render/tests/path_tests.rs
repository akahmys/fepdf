use ferruginous_render::path::PathBuilder;

#[test]
fn test_path_v_y() {
    let mut pb = PathBuilder::new();
    pb.move_to(0.0, 0.0);
    // v: x2 y2 x3 y3 -> p1 = (0,0), p2 = (10, 20), p3 = (30, 40)
    pb.curve_v(10.0, 20.0, 30.0, 40.0);

    let path = pb.finish();
    assert_eq!(path.segments().count(), 1);
}
