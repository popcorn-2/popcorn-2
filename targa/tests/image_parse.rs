use targa::{Image, Pixel};

const VALID_IMAGE: &'static [u8] = include_bytes!("check.tga");

#[test]
fn parses_image() {
	let img = Image::try_new(VALID_IMAGE);
	assert!(img.is_ok());
	let img = img.unwrap();
	assert_eq!(img.width, 54);
	assert_eq!(img.height, 50);
	assert_eq!(img.get_pixel(0, 0), Some(Pixel{r: 43, g: 45, b: 48, a: 255}));
}
