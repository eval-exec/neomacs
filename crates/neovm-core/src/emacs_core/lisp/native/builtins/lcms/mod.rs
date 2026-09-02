use crate::emacs_core::error::{expect_args, expect_args_range};
use std::ffi::c_void;
use std::sync::OnceLock;

use libloading::Library;

use super::*;

mod subrs;
#[cfg(test)]
pub(crate) use self::subrs::SUBRS;
pub(crate) use self::subrs::register_subrs;

#[repr(C)]
#[derive(Clone, Copy)]
struct CmsCIEXYZ {
    x: f64,
    y: f64,
    z: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CmsCIExyY {
    x: f64,
    y: f64,
    yy: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CmsCIELab {
    l: f64,
    a: f64,
    b: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CmsJCh {
    j: f64,
    c: f64,
    h: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CmsViewingConditions {
    white_point: CmsCIEXYZ,
    yb: f64,
    la: f64,
    surround: u32,
    d_value: f64,
}

#[derive(Clone, Copy)]
struct LcmsJab {
    j: f64,
    a: f64,
    b: f64,
}

type CmsCIE2000DeltaE =
    unsafe extern "C" fn(*const CmsCIELab, *const CmsCIELab, f64, f64, f64) -> f64;
type CmsCIECAM02Init =
    unsafe extern "C" fn(*mut c_void, *const CmsViewingConditions) -> *mut c_void;
type CmsCIECAM02Forward = unsafe extern "C" fn(*mut c_void, *const CmsCIEXYZ, *mut CmsJCh);
type CmsCIECAM02Reverse = unsafe extern "C" fn(*mut c_void, *const CmsJCh, *mut CmsCIEXYZ);
type CmsCIECAM02Done = unsafe extern "C" fn(*mut c_void);
type CmsWhitePointFromTemp = unsafe extern "C" fn(*mut CmsCIExyY, f64) -> i32;
type CmsXyY2XYZ = unsafe extern "C" fn(*mut CmsCIEXYZ, *const CmsCIExyY);

struct Lcms {
    _library: Library,
    cie2000_delta_e: CmsCIE2000DeltaE,
    ciecam02_init: CmsCIECAM02Init,
    ciecam02_forward: CmsCIECAM02Forward,
    ciecam02_reverse: CmsCIECAM02Reverse,
    ciecam02_done: CmsCIECAM02Done,
    white_point_from_temp: CmsWhitePointFromTemp,
    xyy_to_xyz: CmsXyY2XYZ,
}

static LCMS: OnceLock<Option<Lcms>> = OnceLock::new();

const ILLUMINANT_D65: CmsCIEXYZ = CmsCIEXYZ {
    x: 95.0455,
    y: 100.0,
    z: 108.8753,
};

fn lcms() -> Option<&'static Lcms> {
    LCMS.get_or_init(load_lcms).as_ref()
}

fn load_lcms() -> Option<Lcms> {
    for name in lcms_library_candidates() {
        let Ok(library) = (unsafe { Library::new(name) }) else {
            continue;
        };
        let Ok(lcms) = (unsafe { symbols_from_library(library) }) else {
            continue;
        };
        return Some(lcms);
    }
    None
}

fn lcms_library_candidates() -> Vec<&'static str> {
    let mut candidates = Vec::new();
    if let Some(build_candidates) = option_env!("NEOMACS_LCMS2_LIBRARY_CANDIDATES") {
        candidates.extend(build_candidates.split(':').filter(|item| !item.is_empty()));
    }
    candidates.extend(std::cfg_select! {
        target_os = "windows" => vec!["liblcms2-2.dll", "lcms2.dll"],
        target_os = "macos" => vec!["liblcms2.2.dylib", "liblcms2.dylib"],
        target_os = "linux" => vec!["liblcms2.so.2", "liblcms2.so"],
        unix => vec!["liblcms2.so.2", "liblcms2.so"],
        _ => vec!["lcms2"],
    });
    candidates
}

unsafe fn symbols_from_library(library: Library) -> Result<Lcms, libloading::Error> {
    let cie2000_delta_e = *unsafe { library.get::<CmsCIE2000DeltaE>(b"cmsCIE2000DeltaE")? };
    let ciecam02_init = *unsafe { library.get::<CmsCIECAM02Init>(b"cmsCIECAM02Init")? };
    let ciecam02_forward = *unsafe { library.get::<CmsCIECAM02Forward>(b"cmsCIECAM02Forward")? };
    let ciecam02_reverse = *unsafe { library.get::<CmsCIECAM02Reverse>(b"cmsCIECAM02Reverse")? };
    let ciecam02_done = *unsafe { library.get::<CmsCIECAM02Done>(b"cmsCIECAM02Done")? };
    let white_point_from_temp =
        *unsafe { library.get::<CmsWhitePointFromTemp>(b"cmsWhitePointFromTemp")? };
    let xyy_to_xyz = *unsafe { library.get::<CmsXyY2XYZ>(b"cmsxyY2XYZ")? };
    Ok(Lcms {
        _library: library,
        cie2000_delta_e,
        ciecam02_init,
        ciecam02_forward,
        ciecam02_reverse,
        ciecam02_done,
        white_point_from_temp,
        xyy_to_xyz,
    })
}

fn lcms_or_nil() -> Result<&'static Lcms, Value> {
    lcms().ok_or(Value::NIL)
}

fn invalid_object(message: &str, value: Value) -> Flow {
    signal("error", vec![Value::string(message), value])
}

fn parse_number(value: Value) -> Option<f64> {
    expect_number(&value).ok()
}

fn parse_three_numbers(mut list: Value, scale: f64, strict_tail: bool) -> Option<[f64; 3]> {
    let mut out = [0.0; 3];
    for item in &mut out {
        if !list.is_cons() {
            return None;
        }
        *item = parse_number(list.cons_car())? * scale;
        list = list.cons_cdr();
    }
    if strict_tail && !list.is_nil() {
        return None;
    }
    Some(out)
}

fn parse_lab_list(value: Value) -> Option<CmsCIELab> {
    let [l, a, b] = parse_three_numbers(value, 1.0, false)?;
    Some(CmsCIELab { l, a, b })
}

fn parse_xyz_list(value: Value) -> Option<CmsCIEXYZ> {
    let [x, y, z] = parse_three_numbers(value, 100.0, false)?;
    Some(CmsCIEXYZ { x, y, z })
}

fn parse_jch_list(value: Value) -> Option<CmsJCh> {
    let [j, c, h] = parse_three_numbers(value, 1.0, true)?;
    Some(CmsJCh { j, c, h })
}

fn parse_jab_list(value: Value) -> Option<LcmsJab> {
    let [j, a, b] = parse_three_numbers(value, 1.0, false)?;
    Some(LcmsJab { j, a, b })
}

fn parse_viewing_conditions(value: Value, white_point: CmsCIEXYZ) -> Option<CmsViewingConditions> {
    let mut tail = value;
    let yb = parse_next_number(&mut tail)?;
    let la = parse_next_number(&mut tail)?;
    if !tail.is_cons() {
        return None;
    }
    let surround = tail.cons_car().as_fixnum()?;
    if !(1..=4).contains(&surround) {
        return None;
    }
    tail = tail.cons_cdr();
    let d_value = parse_next_number(&mut tail)?;
    if !tail.is_nil() {
        return None;
    }

    Some(CmsViewingConditions {
        white_point,
        yb,
        la,
        surround: surround as u32,
        d_value,
    })
}

fn parse_next_number(tail: &mut Value) -> Option<f64> {
    if !tail.is_cons() {
        return None;
    }
    let number = parse_number(tail.cons_car())?;
    *tail = tail.cons_cdr();
    Some(number)
}

fn default_viewing_conditions(white_point: CmsCIEXYZ) -> CmsViewingConditions {
    CmsViewingConditions {
        white_point,
        yb: 20.0,
        la: 100.0,
        surround: 1,
        d_value: 1.0,
    }
}

fn whitepoint_and_view(
    whitepoint: Value,
    view: Value,
    view_error: &'static str,
) -> Result<(CmsCIEXYZ, CmsViewingConditions), Flow> {
    let xyzw = if whitepoint.is_nil() {
        ILLUMINANT_D65
    } else {
        parse_xyz_list(whitepoint)
            .ok_or_else(|| invalid_object("Invalid white point", whitepoint))?
    };
    let vc = if view.is_nil() {
        default_viewing_conditions(xyzw)
    } else {
        parse_viewing_conditions(view, xyzw).ok_or_else(|| invalid_object(view_error, view))?
    };
    Ok((xyzw, vc))
}

fn xyz_to_jch(lcms: &Lcms, xyz: &CmsCIEXYZ, vc: &CmsViewingConditions) -> CmsJCh {
    let mut jch = CmsJCh {
        j: 0.0,
        c: 0.0,
        h: 0.0,
    };
    unsafe {
        let handle = (lcms.ciecam02_init)(std::ptr::null_mut(), vc);
        (lcms.ciecam02_forward)(handle, xyz, &mut jch);
        (lcms.ciecam02_done)(handle);
    }
    jch
}

fn jch_to_xyz(lcms: &Lcms, jch: &CmsJCh, vc: &CmsViewingConditions) -> CmsCIEXYZ {
    let mut xyz = CmsCIEXYZ {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    unsafe {
        let handle = (lcms.ciecam02_init)(std::ptr::null_mut(), vc);
        (lcms.ciecam02_reverse)(handle, jch, &mut xyz);
        (lcms.ciecam02_done)(handle);
    }
    xyz
}

fn deg2rad(degrees: f64) -> f64 {
    std::f64::consts::PI * degrees / 180.0
}

fn rad2deg(radians: f64) -> f64 {
    180.0 * radians / std::f64::consts::PI
}

fn fl_for_view(vc: &CmsViewingConditions) -> f64 {
    let k = 1.0 / (1.0 + (5.0 * vc.la));
    let k4 = k * k * k * k;
    vc.la * k4 + 0.1 * (1.0 - k4) * (1.0 - k4) * (5.0 * vc.la).cbrt()
}

fn jch_to_jab(jch: &CmsJCh, fl: f64, c1: f64, c2: f64) -> LcmsJab {
    let mp = 43.86 * (1.0 + c2 * (jch.c * fl.sqrt().sqrt())).ln();
    LcmsJab {
        j: 1.7 * jch.j / (1.0 + (c1 * jch.j)),
        a: mp * deg2rad(jch.h).cos(),
        b: mp * deg2rad(jch.h).sin(),
    }
}

fn jab_to_jch(jab: &LcmsJab, fl: f64, c1: f64, c2: f64) -> CmsJCh {
    let mut h = rad2deg(jab.b.atan2(jab.a));
    if h < 0.0 {
        h += 360.0;
    }
    let mp = jab.a.hypot(jab.b);
    CmsJCh {
        j: jab.j / (1.0 + c1 * (100.0 - jab.j)),
        c: ((c2 * mp).exp() - 1.0) / (c2 * fl.sqrt().sqrt()),
        h,
    }
}

fn list3_floats(a: f64, b: f64, c: f64) -> Value {
    Value::list(vec![
        Value::make_float(a),
        Value::make_float(b),
        Value::make_float(c),
    ])
}

pub(crate) fn lcms2_available_p(args: Vec<Value>) -> EvalResult {
    expect_args("lcms2-available-p", &args, 0)?;
    Ok(Value::bool(lcms().is_some()))
}

pub(crate) fn lcms_cie_de2000(args: Vec<Value>) -> EvalResult {
    expect_args_range("lcms-cie-de2000", &args, 2, 5)?;
    let Ok(lcms) = lcms_or_nil() else {
        return Ok(Value::NIL);
    };

    let lab1 = parse_lab_list(args[0]).ok_or_else(|| invalid_object("Invalid color", args[0]))?;
    let lab2 = parse_lab_list(args[1]).ok_or_else(|| invalid_object("Invalid color", args[0]))?;
    let kl = if args.get(2).is_none_or(|value| value.is_nil()) {
        1.0
    } else {
        let value = expect_number(&args[2])?;
        if value == 0.0 {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("numberp"), args[2]],
            ));
        }
        value
    };
    let kc = if args.get(3).is_none_or(|value| value.is_nil()) {
        1.0
    } else {
        let value = expect_number(&args[3])?;
        if value == 0.0 {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("numberp"), args[3]],
            ));
        }
        value
    };
    let kh = if args.get(2).is_none_or(|value| value.is_nil()) {
        1.0
    } else {
        let h = args.get(4).copied().unwrap_or(Value::NIL);
        let value = expect_number(&h)?;
        if value == 0.0 {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("numberp"), h],
            ));
        }
        value
    };

    Ok(Value::make_float(unsafe {
        (lcms.cie2000_delta_e)(&lab1, &lab2, kl, kc, kh)
    }))
}

pub(crate) fn lcms_xyz_to_jch(args: Vec<Value>) -> EvalResult {
    expect_args_range("lcms-xyz->jch", &args, 1, 3)?;
    let Ok(lcms) = lcms_or_nil() else {
        return Ok(Value::NIL);
    };
    let xyz = parse_xyz_list(args[0]).ok_or_else(|| invalid_object("Invalid color", args[0]))?;
    let (_, vc) = whitepoint_and_view(
        args.get(1).copied().unwrap_or(Value::NIL),
        args.get(2).copied().unwrap_or(Value::NIL),
        "Invalid viewing conditions",
    )?;
    let jch = xyz_to_jch(lcms, &xyz, &vc);
    Ok(list3_floats(jch.j, jch.c, jch.h))
}

pub(crate) fn lcms_jch_to_xyz(args: Vec<Value>) -> EvalResult {
    expect_args_range("lcms-jch->xyz", &args, 1, 3)?;
    let Ok(lcms) = lcms_or_nil() else {
        return Ok(Value::NIL);
    };
    let jch = parse_jch_list(args[0]).ok_or_else(|| invalid_object("Invalid color", args[0]))?;
    let (_, vc) = whitepoint_and_view(
        args.get(1).copied().unwrap_or(Value::NIL),
        args.get(2).copied().unwrap_or(Value::NIL),
        "Invalid viewing conditions",
    )?;
    let xyz = jch_to_xyz(lcms, &jch, &vc);
    Ok(list3_floats(xyz.x / 100.0, xyz.y / 100.0, xyz.z / 100.0))
}

pub(crate) fn lcms_jch_to_jab(args: Vec<Value>) -> EvalResult {
    expect_args_range("lcms-jch->jab", &args, 1, 3)?;
    if lcms().is_none() {
        return Ok(Value::NIL);
    }
    let jch = parse_jch_list(args[0]).ok_or_else(|| invalid_object("Invalid color", args[0]))?;
    let (_, vc) = whitepoint_and_view(
        args.get(1).copied().unwrap_or(Value::NIL),
        args.get(2).copied().unwrap_or(Value::NIL),
        "Invalid viewing conditions",
    )?;
    let jab = jch_to_jab(&jch, fl_for_view(&vc), 0.007, 0.0228);
    Ok(list3_floats(jab.j, jab.a, jab.b))
}

pub(crate) fn lcms_jab_to_jch(args: Vec<Value>) -> EvalResult {
    expect_args_range("lcms-jab->jch", &args, 1, 3)?;
    if lcms().is_none() {
        return Ok(Value::NIL);
    }
    let jab = parse_jab_list(args[0]).ok_or_else(|| invalid_object("Invalid color", args[0]))?;
    let (_, vc) = whitepoint_and_view(
        args.get(1).copied().unwrap_or(Value::NIL),
        args.get(2).copied().unwrap_or(Value::NIL),
        "Invalid viewing conditions",
    )?;
    let jch = jab_to_jch(&jab, fl_for_view(&vc), 0.007, 0.0228);
    Ok(list3_floats(jch.j, jch.c, jch.h))
}

pub(crate) fn lcms_cam02_ucs(args: Vec<Value>) -> EvalResult {
    expect_args_range("lcms-cam02-ucs", &args, 2, 4)?;
    let Ok(lcms) = lcms_or_nil() else {
        return Ok(Value::NIL);
    };
    let xyz1 = parse_xyz_list(args[0]).ok_or_else(|| invalid_object("Invalid color", args[0]))?;
    let xyz2 = parse_xyz_list(args[1]).ok_or_else(|| invalid_object("Invalid color", args[1]))?;
    let (_, vc) = whitepoint_and_view(
        args.get(2).copied().unwrap_or(Value::NIL),
        args.get(3).copied().unwrap_or(Value::NIL),
        "Invalid view conditions",
    )?;
    let jch1 = xyz_to_jch(lcms, &xyz1, &vc);
    let jch2 = xyz_to_jch(lcms, &xyz2, &vc);
    let fl = fl_for_view(&vc);
    let jab1 = jch_to_jab(&jch1, fl, 0.007, 0.0228);
    let jab2 = jch_to_jab(&jch2, fl, 0.007, 0.0228);
    Ok(Value::make_float(
        (jab2.j - jab1.j).hypot((jab2.a - jab1.a).hypot(jab2.b - jab1.b)),
    ))
}

pub(crate) fn lcms_temp_to_white_point(args: Vec<Value>) -> EvalResult {
    expect_args("lcms-temp->white-point", &args, 1)?;
    let Ok(lcms) = lcms_or_nil() else {
        return Ok(Value::NIL);
    };
    let temp_k = expect_number(&args[0])?;
    let mut whitepoint = CmsCIExyY {
        x: 0.0,
        y: 0.0,
        yy: 0.0,
    };
    let ok = unsafe { (lcms.white_point_from_temp)(&mut whitepoint, temp_k) };
    if ok == 0 {
        return Err(invalid_object("Invalid temperature", args[0]));
    }
    let mut xyz = CmsCIEXYZ {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    unsafe {
        (lcms.xyy_to_xyz)(&mut xyz, &whitepoint);
    }
    Ok(list3_floats(xyz.x, xyz.y, xyz.z))
}
