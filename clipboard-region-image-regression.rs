/// Review regression: copying a region fill must carry its image bytes to another document.
#[test]
fn review_copying_region_image_preserves_image_payload() {
    let mut s = Session::new();
    let id = rect(&mut s);
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([200, 40, 40, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let key;
    {
        let doc = s.doc_mut().unwrap();
        key = figcraft_format::add_image(doc, png).unwrap().0;
        let node = doc.get_mut(id).unwrap();
        node.fills.clear();
        let NodeKind::Vector { network } = &mut node.kind else { panic!("vector") };
        network.regions[0].fills = Some(vec![figcraft_color::Paint::image(key.clone(), figcraft_color::ImageScaleMode::Fill)]);
    }
    let n = net(&s, id);
    ex(&mut s, "vector.edit.select", json!({"vertices": (0..n.vertices.len()).collect::<Vec<_>>()}));
    ex(&mut s, "edit.copy", json!({}));
    assert!(s.clipboard.images.contains_key(&key), "the copied region references the image, but the clipboard does not carry it");
}
