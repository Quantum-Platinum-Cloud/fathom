extern crate ddl_edid;

use std::io::Cursor;
use ddl_edid::Edid;

const MAGIC: [u8; 8] = [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00];

#[test]
fn test_read_edid_mbp_2013_built_in_retina() {
    let buf = include_bytes!("fixtures/mbp_2013_built_in_retina.bin");
    let edid = Edid::read(&mut Cursor::new(&buf[..])).unwrap();

    assert_eq!(edid.header.magic, MAGIC);
    assert_eq!(edid.header.mfg_bytes, 4102);
    assert_eq!(edid.header.product_code, 40985);
    assert_eq!(edid.header.serial, 0);
    assert_eq!(edid.header.mfg_week, 48);
    assert_eq!(edid.header.mfg_year_mod, 22); // 1990+22 = 2012
    assert_eq!(edid.header.mfg_year, 2012);
    assert_eq!(edid.header.edid_version_major, 1);
    assert_eq!(edid.header.edid_version_minor, 4);

    // TODO: ...

    // panic!("{:#?}", edid);
}

#[test]
fn test_read_edid_mbp_2017_built_in_retina() {
    let buf = include_bytes!("fixtures/mbp_2017_built_in_retina.bin");
    let edid = Edid::read(&mut Cursor::new(&buf[..])).unwrap();

    assert_eq!(edid.header.magic, MAGIC);

    // TODO: ...

    // panic!("{:#?}", edid);
}

#[test]
fn test_read_edid_yamakasi_0270led() {
    let buf = include_bytes!("fixtures/yamakasi_0270led.bin");
    let edid = Edid::read(&mut Cursor::new(&buf[..])).unwrap();

    assert_eq!(edid.header.magic, MAGIC);

    // TODO: ...

    // panic!("{:#?}", edid);
}
