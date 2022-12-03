use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use ndarray::{Array1, ArrayView2};
use crate::apbs_param::*;
use crate::Parameters;

pub fn write_apbs(ndx_rec: &Vec<usize>, ndx_lig: &Vec<usize>, coord: &ArrayView2<f64>,
                  atm_radius: &Array1<f64>, pbe_set: &PBESet, pba_set: &PBASet,
                  temp_dir: &PathBuf, f_name: &String, settings: &Parameters) {
    let mut input_apbs = File::create(temp_dir.join(format!("{}.apbs", f_name))).unwrap();
    input_apbs.write_all("read\n".as_bytes()).expect("Failed to write apbs input file.");
    input_apbs.write_all(format!("  mol pqr {0}_com.pqr\
    \n  mol pqr {0}_rec.pqr\
    \n  mol pqr {0}_lig.pqr\
    \nend\n\n", f_name).as_bytes())
        .expect("Failed to write apbs input file.");

    let (rec_box, lig_box, com_box) =
        gen_mesh_params(ndx_rec, ndx_lig, coord, atm_radius);

    let mut pbe_set0 = PBESet::from(pbe_set);
    pbe_set0.sdie = 1.0;

    input_apbs.write_all(dim_apbs(format!("{}_com", f_name).as_str(), 1,
                                  com_box[0], com_box[3], com_box[1],
                                  com_box[4], com_box[2], com_box[5],
                                  settings,
                                  pbe_set, &pbe_set0, pba_set).as_bytes()).
        expect("Failed writing apbs file.");
    input_apbs.write_all(dim_apbs(format!("{}_rec", f_name).as_str(), 2,
                                  rec_box[0], rec_box[3], rec_box[1],
                                  rec_box[4], rec_box[2], rec_box[5],
                                  settings,
                                  pbe_set, &pbe_set0, pba_set).as_bytes()).
        expect("Failed writing apbs file.");
    input_apbs.write_all(dim_apbs(format!("{}_lig", f_name).as_str(), 3,
                                  lig_box[0], lig_box[3], lig_box[1],
                                  lig_box[4], lig_box[2], lig_box[5],
                                  settings,
                                  pbe_set, &pbe_set0, pba_set).as_bytes()).
        expect("Failed writing apbs file.");
}

fn get_lb(ndx: &Vec<usize>, axis: usize, coord: &ArrayView2<f64>, atm_radius: &Array1<f64>) -> f64 {
    ndx.iter().map(|&p| coord[[p, axis]] - atm_radius[p])
        .fold(f64::INFINITY, |prev, curr| prev.min(curr))
}

fn get_ub(ndx: &Vec<usize>, axis: usize, coord: &ArrayView2<f64>, atm_radius: &Array1<f64>) -> f64 {
    ndx.iter().map(|&p| coord[[p, axis]] + atm_radius[p])
        .fold(f64::NEG_INFINITY, |prev, curr| prev.max(curr))
}

pub fn gen_mesh_params(ndx_rec: &Vec<usize>, ndx_lig: &Vec<usize>, coord: &ArrayView2<f64>,
                       atm_radius: &Array1<f64>) -> ([f64; 6], [f64; 6], [f64; 6]) {
    // border of the whole molecule
    // let min_x = coordinates.slice(s![.., .., 0]).iter().
    //     fold(f64::INFINITY, |prev, curr| prev.min(*curr));
    // let max_x = coordinates.slice(s![.., .., 0]).iter().
    //     fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));
    // let min_y = coordinates.slice(s![.., .., 1]).iter().
    //     fold(f64::INFINITY, |prev, curr| prev.min(*curr));
    // let max_y = coordinates.slice(s![.., .., 1]).iter().
    //     fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));
    // let min_z = coordinates.slice(s![.., .., 2]).iter().
    //     fold(f64::INFINITY, |prev, curr| prev.min(*curr));
    // let max_z = coordinates.slice(s![.., .., 2]).iter().
    //     fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));

    let min_x_rec = get_lb(ndx_rec, 0, coord, atm_radius);
    let min_y_rec = get_lb(ndx_rec, 1, coord, atm_radius);
    let min_z_rec = get_lb(ndx_rec, 2, coord, atm_radius);
    let max_x_rec = get_ub(ndx_rec, 0, coord, atm_radius);
    let max_y_rec = get_ub(ndx_rec, 1, coord, atm_radius);
    let max_z_rec = get_ub(ndx_rec, 2, coord, atm_radius);

    let rec_box = [min_x_rec, min_y_rec, min_z_rec, max_x_rec, max_y_rec, max_z_rec];

    let min_x_lig = get_lb(ndx_lig, 0, coord, atm_radius);
    let min_y_lig = get_lb(ndx_lig, 1, coord, atm_radius);
    let min_z_lig = get_lb(ndx_lig, 2, coord, atm_radius);
    let max_x_lig = get_ub(ndx_lig, 0, coord, atm_radius);
    let max_y_lig = get_ub(ndx_lig, 1, coord, atm_radius);
    let max_z_lig = get_ub(ndx_lig, 2, coord, atm_radius);

    let lig_box = [min_x_lig, min_y_lig, min_z_lig, max_x_lig, max_y_lig, max_z_lig];

    let min_x_com = min_x_rec.min(min_x_lig);
    let min_y_com = min_y_rec.min(min_y_lig);
    let min_z_com = min_z_rec.min(min_z_lig);
    let max_x_com = max_x_rec.max(max_x_lig);
    let max_y_com = max_y_rec.max(max_y_lig);
    let max_z_com = max_z_rec.max(max_z_lig);

    let com_box = [min_x_com, min_y_com, min_z_com, max_x_com, max_y_com, max_z_com];

    return (rec_box, lig_box, com_box);

    // if mesh_type == 0 {
    //     // GMXPBSA
    //     input_apbs.write_all(dim_apbs(format!("{}_com", f_name).as_str(), 1,
    //                                   min_x, max_x, min_y,
    //                                   max_y, min_z, max_z,
    //                                   settings,
    //                                   &pbe_set, &pbe_set0, &pba_set).as_bytes()).
    //         expect("Failed writing apbs file.");
    //     input_apbs.write_all(dim_apbs(format!("{}_rec", f_name).as_str(), 2,
    //                                   min_x, max_x, min_y,
    //                                   max_y, min_z, max_z,
    //                                   settings,
    //                                   &pbe_set, &pbe_set0, &pba_set).as_bytes()).
    //         expect("Failed writing apbs file.");
    //     input_apbs.write_all(dim_apbs(format!("{}_lig", f_name).as_str(), 3,
    //                                   min_x, max_x, min_y,
    //                                   max_y, min_z, max_z,
    //                                   settings,
    //                                   &pbe_set, &pbe_set0, &pba_set).as_bytes()).
    //         expect("Failed writing apbs file.");
    // } else if mesh_type == 1 {
    // write apbs input files for g_mmpbsa
    // }
}

pub fn dim_apbs(file: &str, mol_index: i32, min_x: f64, max_x: f64, min_y: f64, max_y: f64, min_z: f64, max_z: f64,
                settings: &Parameters, pbe_set: &PBESet, pbe_set0: &PBESet, pba_set: &PBASet) -> String {
    let cfac = settings.cfac;
    let fadd = settings.fadd;
    let df = settings.df;

    // convert to A
    let min_x = min_x * 10.0;
    let min_y = min_y * 10.0;
    let min_z = min_z * 10.0;
    let max_x = max_x * 10.0;
    let max_y = max_y * 10.0;
    let max_z = max_z * 10.0;

    let x_len = (max_x - min_x).max(0.1);
    let x_center = (max_x + min_x) / 2.0;
    let y_len = (max_y - min_y).max(0.1);
    let y_center = (max_y + min_y) / 2.0;
    let z_len = (max_z - min_z).max(0.1);
    let z_center = (max_z + min_z) / 2.0;

    let c_x = x_len * cfac;
    let c_y = y_len * cfac;
    let c_z = z_len * cfac;
    let f_x = (x_len + fadd).min(c_x);
    let f_y = (y_len + fadd).min(c_y);
    let f_z = (z_len + fadd).min(c_z);
    let n_x = (f_x / df).round() as i32;
    let n_y = (f_y / df).round() as i32;
    let n_z = (f_z / df).round() as i32;

    let mg_set = "mg-auto";

    let xyz_set = format!("  {mg_set}\n  mol {mol_index}\
        \n  dime   {n_x}  {n_y}  {n_z}\
        \n  cglen  {c_x:.3}  {c_y:.3}  {c_z:.3}\
        \n  fglen  {f_x:.3}  {f_y:.3}  {f_z:.3}\
        \n  fgcent {x_center:.3}  {y_center:.3}  {z_center:.3}\
        \n  cgcent {x_center:.3}  {y_center:.3}  {z_center:.3}\n");

    return format!("\nELEC name {}\n\
    {} \n\
    {} \n\
    end\n\n\
    ELEC name {}_VAC\n\
    {}\n\
    {} \n\
    end\n\n\
    APOLAR name {}_SAS\n  \
    mol {}\n{}\n\
    end\n\n\
    print elecEnergy {} - {}_VAC end\n\
    print apolEnergy {}_SAS end\n\n", file, xyz_set, pbe_set.to_string(), file,
                   xyz_set, pbe_set0.to_string(), file, mol_index, pba_set.to_string(),
                   file, file, file);
}