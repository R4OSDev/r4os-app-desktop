//! Source-only freestanding consumer of the actual current ported owners.
//! The runner appends this entry to a temporary copy of their crate root;
//! no Desktop host, display/Present, input-latency or performance claim.
use r4os_sdk::{App,path::FilePath,storage::System};
use crate::render::{geometry::Rect,pixels::PixelBytes,primitive_assets::Cache,primitive_image::{Format,View},scene::{CpuScene,Mapping,Xrgb32Nearest}};
fn exercise()->Result<(),i32> {
    let mut pixels=alloc::vec![0x5a123456u32;2050];
    {
        let mut scene=CpuScene::new(&mut pixels[1..2049],64,32).ok_or(10)?;
        scene.clear_layer(scene.full_rect());if scene.pixels().iter().any(|p|*p!=0){return Err(11);}
        let source=[0x112233,0x445566,0x778899,0xaabbcc];
        if !scene.blit_xrgb32_nearest(Rect::new(3,4,4,4),Xrgb32Nearest{pixels:PixelBytes::from_words(&source),mapping:Mapping{source_x:0,source_y:0,source_w:2,source_h:2,source_stride:2,guest_w:2,guest_h:2,viewport:Rect::new(3,4,4,4)}}){return Err(12);}
        if scene.pixels()[4*64+3]!=0x112233 || scene.pixels()[7*64+6]!=0xaabbcc || scene.pixels()[3*64+3]!=0{return Err(13);}
    }
    if pixels[0]!=0x5a123456 || pixels[2049]!=0x5a123456{return Err(14);}
    let words=[0x123456,0xabcdef,0xeeeeee,0x102030,0x405060,0xdddddd];
    let view=View::new(Format::Xrgb,2,2,12,PixelBytes::from_words(&words).bytes());
    let mut cache=Cache::new(2*1024*1024).map_err(|_|15)?;cache.start(1).map_err(|_|16)?;let entry=cache.intern(view).map_err(|_|17)?;
    let expected=[0x7f,0x90,0xa8,0x70,0x07,0x02,0x95,0x99,0xae,0x01,0xc0,0x71,0xb0,0xa3,0x5d,0x2d,0x97,0xb6,0x02,0x1c,0x64,0x5a,0x33,0x82,0xac,0x93,0x90,0xf8,0x5d,0x8e,0xbe,0x4e];
    if entry.key!=expected{return Err(18);}let converted=cache.converted_pixels;cache.intern(view).map_err(|_|19)?;if cache.converted_pixels!=converted || cache.hits!=1{return Err(20);}
    let mut buffer=[0u8;128];let mut field=crate::shell::run_editor::Editor::new(&mut buffer).map_err(|_|21)?;
    if !field.set("C:\\ä\\中\\A\u{308}.R4X".as_bytes()).map_err(|_|22)?{return Err(23);}
    for _ in 0..5 {if !field.backspace().map_err(|_|24)?{return Err(25);}}
    if field.text().map_err(|_|26)?!="C:\\ä\\中\\".as_bytes(){return Err(27);}
    let mut label=[0;8];if crate::shell::run_editor::copy_tail_label(&mut label,"prefixA\u{308}中".as_bytes(),4)!="..中".as_bytes(){return Err(28);}
    if r4os_sdk::bounded_text::next_boundary("A\u{308}中".as_bytes(),0)!=Some(3) || r4os_sdk::bounded_text::previous_boundary("A\u{308}中".as_bytes(),6)!=Some(3){return Err(29);}
    if crate::hosted::console_scroll::count_lines("1234567ü─".as_bytes(),8,437)!=2{return Err(30);}
    crate::render::shapes::conformance_probe();
    Ok(())
}
fn r4_core_main(app:&mut App<'_>)->i32 {
    let code=exercise().err().unwrap_or(0);
    let mut report=*b"RUST86 core result=000 wide-clear/NN/hash/SDK-graphemes/shapes\r\n";
    report[19]=b'0'+(code/100) as u8;report[20]=b'0'+((code/10)%10) as u8;report[21]=b'0'+(code%10) as u8;
    let Ok(system)=System::new(app) else{return 91;};let Ok(path)=FilePath::parse(b"C:\\TEMP\\D86CORE.TXT") else{return 92;};
    match system.files().write(&path,&report) {Ok(n) if n==report.len()=>code,_=>93}
}
r4os_sdk::app!(r4_core_main);
