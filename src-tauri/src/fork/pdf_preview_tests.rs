use super::pdf_preview::render;

#[test]
fn pdf_pages_render_actual_images_and_reject_bad_input() {
    use lopdf::{dictionary, Document, Object, Stream};
    let mut doc = Document::with_version("1.5");
    let pages = doc.new_object_id();
    let font =
        doc.add_object(dictionary! {"Type"=>"Font","Subtype"=>"Type1","BaseFont"=>"Helvetica"});
    let mut children = Vec::<Object>::new();
    for (label, color) in [
        ("First page", "0.1 0.4 0.8"),
        ("Second page", "0.8 0.2 0.1"),
    ] {
        let content = doc.add_object(Stream::new(
            dictionary! {},
            format!(
                "{color} rg 40 500 500 180 re f 0 0 0 rg BT /F1 32 Tf 50 720 Td ({label}) Tj ET"
            )
            .into_bytes(),
        ));
        let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"Contents"=>content,"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font}},"MediaBox"=>vec![0.into(),0.into(),612.into(),792.into()]});
        children.push(page.into());
    }
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {"Type"=>"Pages","Kids"=>children,"Count"=>2}),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    doc.trailer.set("Root", catalog);
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    let first = render(&bytes, 1).unwrap();
    let second = render(&bytes, 2).unwrap();
    assert_eq!(first.count, 2);
    assert_eq!(second.count, 2);
    assert!(first.png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_ne!(first.png, second.png);
    let width = u32::from_be_bytes(first.png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(first.png[20..24].try_into().unwrap());
    assert_eq!(width, 1600);
    assert!((2000..=2200).contains(&height));
    assert!(render(&bytes, 0).is_err());
    assert!(render(&bytes, 3).is_err());
    assert!(render(b"%PDF- broken", 1).is_err());
    if let Ok(path) = std::env::var("SKIM_TEST_PDF_OUTPUT") {
        std::fs::write(format!("{path}.pdf"), bytes).unwrap();
        std::fs::write(format!("{path}.png"), first.png).unwrap();
    }
}
