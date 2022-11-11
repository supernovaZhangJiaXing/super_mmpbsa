use std::fs;
use std::path::Path;
use crate::index_parser::Index;
use xdrfile::*;
use crate::{get_input_value, Parameters};
use ndarray::{Array1, Array2, Array3, s};
use std::fs::File;
use std::io::{Read, stdin, Write};
use std::process::Command;
use std::rc::Rc;
use indicatif::ProgressBar;
use crate::gen_qrv::gen_qrv;

pub fn do_mmpbsa_calculations(trj: &String, tpr: &String, mdp: &String, ndx: &Index, wd: &Path,
                              use_dh: bool, use_ts: bool,
                              complex_grp: usize, receptor_grp: usize, ligand_grp: usize,
                              bt: f64, et: f64, dt: f64, settings: &Parameters)
                              -> (f64, f64, f64, f64, f64, f64, f64, f64, f64) {
    let mut sys_name = String::from("_system");
    println!("Input system name (default: {}):", sys_name);
    let mut input = String::new();
    stdin().read_line(&mut input).expect("Error input");
    if input.trim().len() != 0 {
        sys_name = input.trim().to_string();
    }
    let qrv_path = String::from(sys_name.as_str()) + ".qrv";
    let qrv_path = wd.join(qrv_path);

    let mut re_gen_qrv = true;
    if qrv_path.is_file() {
        let mdp_sha = gen_file_sha256(mdp);
        let qrv_sha = gen_file_sha256(qrv_path.as_path());
        if wd.join("_mdp.sha").is_file() && wd.join("_qrv.sha").is_file() {
            let old_mdp_sha = fs::read_to_string(wd.join(".mdp.sha")).unwrap();
            let old_qrv_sha = fs::read_to_string(wd.join(".qrv.sha")).unwrap();
            if mdp_sha.eq(&old_mdp_sha) && qrv_sha.eq(&old_qrv_sha) {
                println!("{}", mdp_sha);
                println!("{}", old_mdp_sha);
                println!("{}", qrv_sha);
                println!("{}", old_qrv_sha);
                re_gen_qrv = false;
                println!("Found checked {}. Will not regenerate parameters qrv file.", qrv_path.to_str().unwrap());
            } else {
                println!("Parameter qrv file has been changed.");
            }
        }
    }
    if re_gen_qrv {
        // get charge, radius, LJ parameters of each atoms and generate qrv files
        gen_qrv(mdp, tpr, ndx, wd, receptor_grp, ligand_grp, qrv_path.as_path(), settings);
    }
    // pdb>pqr, output apbs, calculate MM, calculate APBS
    let results = do_mmpbsa(trj, mdp, ndx, qrv_path.to_str().unwrap(), wd, sys_name.as_str(),
                            complex_grp, receptor_grp, ligand_grp,
                            bt, et, dt,
                            settings, use_dh, use_ts);
    return results;
}

fn get_atoms_trj(frames: &Vec<Rc<Frame>>) -> (Array3<f64>, Array3<f64>) {
    let num_frames = frames.len();
    let num_atoms = frames[0].num_atoms();
    let mut coord_matrix = Array3::<f64>::zeros((num_frames, num_atoms, 3));
    let mut box_size = Array3::<f64>::zeros((num_frames, 3, 3));
    for (idx, frame) in frames.into_iter().enumerate() {
        let atoms = frame.coords.to_vec();
        for (i, a) in atoms.into_iter().enumerate() {
            for j in 0..3 {
                coord_matrix[[idx, i, j]] = a[j] as f64;
            }
        }
        for (i, b) in frame.box_vector.into_iter().enumerate() {
            for j in 0..3 {
                box_size[[idx, i, j]] = b[j] as f64;
            }
        }
    }
    return (coord_matrix, box_size);
}

fn do_mmpbsa(trj: &String, mdp: &String, ndx: &Index, qrv: &str, wd: &Path, sys_name: &str,
             complex_grp: usize, receptor_grp: usize, ligand_grp: usize,
             bt: f64, et: f64, dt: f64,
             settings: &Parameters, use_dh: bool, use_ts: bool)
             -> (f64, f64, f64, f64, f64, f64, f64, f64, f64) {
    // Running settings
    let apbs = &settings.apbs;
    let mesh_type = settings.mesh_type;
    let grid_type = settings.grid_type;
    let cfac = settings.cfac;
    let fadd = settings.fadd;
    let df = settings.df;

    // PB部分的参数从哪来?是否从前一菜单转移?
    let PBEset0 = "  temp  298.15      # 温度\
    \n  pdie  2           # 溶质介电常数\
    \n  sdie  1           # 溶剂介电常数, 真空1, 水78.54\
    \n  \
    \n  npbe              # PB方程求解方法, lpbe(线性), npbe(非线性), smbpe(大小修正)\
    \n  bcfl  mdh         # 粗略格点PB方程的边界条件, zero, sdh/mdh(single/multiple Debye-Huckel), focus, map\
    \n  srfm  smol        # 构建介质和离子边界的模型, mol(分子表面), smol(平滑分子表面), spl2/4(三次样条/7阶多项式)\
    \n  chgm  spl4        # 电荷映射到格点的方法, spl0/2/4, 三线性插值, 立方/四次B样条离散\
    \n  swin  0.3         # 立方样条的窗口值, 仅用于 srfm=spl2/4\
    \n  \
    \n  srad  1.4         # 溶剂探测半径\
    \n  sdens 10          # 表面密度, 每A^2的格点数, (srad=0)或(srfm=spl2/4)时不使用\
    \n  \
    \n  ion charge  1 conc 0.15 radius 0.95  # 阳离子的电荷, 浓度, 半径\
    \n  ion charge -1 conc 0.15 radius 1.81  # 阴离子\
    \n  \
    \n  calcforce  no\
    \n  calcenergy comps";
    let PBEset = "  temp  298.15      # 温度\
    \n  pdie  2           # 溶质介电常数\
    \n  sdie  78.54       # 溶剂介电常数, 真空1, 水78.54\
    \n  \
    \n  npbe              # PB方程求解方法, lpbe(线性), npbe(非线性), smbpe(大小修正)\
    \n  bcfl  mdh         # 粗略格点PB方程的边界条件, zero, sdh/mdh(single/multiple Debye-Huckel), focus, map\
    \n  srfm  smol        # 构建介质和离子边界的模型, mol(分子表面), smol(平滑分子表面), spl2/4(三次样条/7阶多项式)\
    \n  chgm  spl4        # 电荷映射到格点的方法, spl0/2/4, 三线性插值, 立方/四次B样条离散\
    \n  swin  0.3         # 立方样条的窗口值, 仅用于 srfm=spl2/4\
    \n  \
    \n  srad  1.4         # 溶剂探测半径\
    \n  sdens 10          # 表面密度, 每A^2的格点数, (srad=0)或(srfm=spl2/4)时不使用\
    \n  \
    \n  ion charge  1 conc 0.15 radius 0.95  # 阳离子的电荷, 浓度, 半径\
    \n  ion charge -1 conc 0.15 radius 1.81  # 阴离子\
    \n  \
    \n  calcforce  no\
    \n  calcenergy comps";
    let PBAset = "  temp  298.15 # 温度\
    \n  srfm  sacc   # 构建溶剂相关表面或体积的模型\
    \n  swin  0.3    # 立方样条窗口(A), 用于定义样条表面\
    \n  \
    \n  # SASA\
    \n  srad  1.4    # 探测半径(A)\
    \n  gamma 1      # 表面张力(kJ/mol-A^2)\
    \n  \
    \n  #gamma const 0.0226778 3.84928\
    \n  #gamma const 0.027     0\
    \n  #gamma const 0.0301248 0         # AMBER-PB4 .0072*cal2J 表面张力, 常数\
    \n  \
    \n  press  0     # 压力(kJ/mol-A^3)\
    \n  bconc  0     # 溶剂本体密度(A^3)\
    \n  sdens 10\
    \n  dpos  0.2\
    \n  grid  0.1 0.1 0.1\
    \n  \
    \n  # SAV\
    \n  #srad  1.29      # SAV探测半径(A)\
    \n  #press 0.234304  # 压力(kJ/mol-A^3)\
    \n  \
    \n  # WCA\
    \n  #srad   1.25           # 探测半径(A)\
    \n  #sdens  200            # 表面的格点密度(1/A)\
    \n  #dpos   0.05           # 表面积导数的计算步长\
    \n  #bconc  0.033428       # 溶剂本体密度(A^3)\
    \n  #grid   0.45 0.45 0.45 # 算体积分时的格点间距(A)\
    \n  \
    \n  calcforce no\
    \n  calcenergy total";

    let qrv = wd.join(sys_name.to_string() + ".qrv");
    let wd = wd.join(sys_name);
    println!("Temporary files will be placed at {}", wd.display());
    if !wd.is_dir() {
        fs::create_dir(&wd).expect(format!("Failed to create temp directory: {}.", sys_name).as_str());
    } else {
        println!("Directory {} not empty. Clear? [Y/n]", wd.display());
        let mut input = String::from("");
        stdin().read_line(&mut input).expect("Get input error");
        if input.trim().len() == 0 || input.trim() == "Y" || input.trim() == "y" {
            fs::remove_dir_all(&wd).expect("Remove dir failed");
            fs::create_dir(&wd).expect(format!("Failed to create temp directory: {}.", sys_name).as_str());
        }
    }

    // run MM-PBSA calculatons
    println!("Running MM-PBSA calculatons...");
    let qrv = fs::read_to_string(qrv).unwrap();
    let qrv: Vec<&str> = qrv.split("\n").collect();
    // c6 and c12
    let Atyp = qrv[1];
    let Atyp: usize = Atyp.trim().parse().unwrap();
    let mut C6 = Array2::<f64>::zeros((Atyp, Atyp));
    let mut C12 = Array2::<f64>::zeros((Atyp, Atyp));
    for i in 0..Atyp {
        for j in 0..Atyp {
            let paralj: Vec<&str> = qrv[i + 2].trim().split(" ").collect();
            let c6: f64 = paralj[2 * j + 1].parse().unwrap();
            C6[[i, j]] = c6;
            let c12: f64 = paralj[2 * j + 2].parse().unwrap();
            C12[[i, j]] = c12;
        }
    }

    let ndx_com = &ndx.groups[complex_grp].indexes;
    let ndx_rec = &ndx.groups[receptor_grp].indexes;
    let ndx_lig = &ndx.groups[ligand_grp].indexes;
    let mut atm_charge = Array1::<f64>::zeros(qrv.len() - 3 - Atyp);
    let mut atm_radius = Array1::<f64>::zeros(qrv.len() - 3 - Atyp);
    let mut atm_typeindex = Array1::<usize>::zeros(qrv.len() - 3 - Atyp);
    let mut atm_sigma = Array1::<f64>::zeros(qrv.len() - 3 - Atyp);
    let mut atm_epsilon = Array1::<f64>::zeros(qrv.len() - 3 - Atyp);
    let mut atm_index = Array1::<i32>::zeros(qrv.len() - 3 - Atyp);
    let mut atm_name = Array1::<String>::default(qrv.len() - 3 - Atyp);
    let mut atm_resname = Array1::<String>::default(qrv.len() - 3 - Atyp);
    let mut atm_resnum = Array1::<usize>::zeros(qrv.len() - 3 - Atyp);
    // res_rec has blanks in the left part
    let mut res_rec = Array1::<String>::default(qrv.len() - 3 - Atyp);
    // res_lig has blanks in the right part
    let mut res_lig = Array1::<String>::default(qrv.len() - 3 - Atyp);
    let mut idx = 0;
    for line in &qrv[Atyp + 2..qrv.len() - 1] { // there's a blank line at the end
        let line: Vec<&str> = line
            .split(" ")
            .filter_map(|p| match p.len() {
                0 => None,
                _ => Some(p)
            })
            .collect();
        atm_charge[idx] = line[1].parse().unwrap();
        atm_radius[idx] = line[2].parse().unwrap();
        atm_typeindex[idx] = line[3].parse().unwrap();
        atm_sigma[idx] = line[4].parse().unwrap();
        atm_epsilon[idx] = line[5].parse().unwrap();
        atm_index[idx] = line[0].parse().unwrap();
        atm_name[idx] = line[line.len() - 2].to_string();
        let res = line[line.len() - 3];
        atm_resnum[idx] = res[0..5].parse().unwrap();
        atm_resname[idx] = res[5..].to_string();

        if line[line.len() - 1] == "Rec" {
            res_rec[idx] = "R~".to_string() + line[line.len() - 3];
        } else {
            res_lig[idx] = "L~".to_string() + line[line.len() - 3];
        }
        idx += 1;
    }

    // fix resnum as start at 0 and no leap
    let diff = &atm_resnum.slice(s![1..atm_resnum.len() - 1]) - &atm_resnum.slice(s![0..atm_resnum.len() - 2]);
    let mut boundaries: Vec<usize> = vec![0];
    for (idx, res_num) in diff.into_iter().enumerate() {
        if res_num > 0 {
            boundaries.push(idx + 1);
        }
    }
    boundaries.push(atm_resnum.len());
    for b in 1..boundaries.len() {
        for i in boundaries[b - 1]..boundaries[b] {
            atm_resnum[i] = b - 1;
        }
    }

    // atom number of receptor and ligand
    let atom_num_rec = ndx_rec.len();
    let atom_num_lig = ndx_lig.len();
    let atom_num_com = atom_num_rec + atom_num_lig;

    // PBSA parameters
    let mut temp = 0.0;
    let mut pdie = 0.0;
    let mut sdie = 0.0;
    let mut Nion: usize = 0;
    let mut Qion = Array1::<f64>::zeros(10);
    let mut Cion = Array1::<f64>::zeros(10);

    // polar parameters
    for i in PBEset.split("\n") {
        if i.trim().len() != 0 {
            let paras: Vec<&str> = i
                .split(" ")
                .filter_map(|p| match p.len() {
                    0 => None,
                    _ => Some(p)
                })
                .collect();
            match paras[0] {
                "temp" => temp = paras[1].parse().unwrap(),
                "pdie" => pdie = paras[1].parse().unwrap(),
                "sdie" => sdie = paras[1].parse().unwrap(),
                "ion" => {
                    Nion += 1;
                    Qion[Nion] = paras[2].parse().unwrap();
                    Cion[Nion] = paras[4].parse().unwrap();
                }
                _ => ()
            }
        }
    }

    let mut gamma = 0.0;
    let mut _const = 0.0;
    // apolar parameters
    for i in PBAset.split("\n") {
        if i.trim().len() != 0 {
            let paras: Vec<&str> = i
                .split(" ")
                .filter_map(|p| match p.len() {
                    0 => None,
                    _ => Some(p)
                })
                .collect();
            if paras[0].ends_with("gamma") && paras[1] == "const" {
                gamma = paras[2].parse().unwrap();
                _const = paras[3].parse().unwrap();
            }
        }
    }
    println!("Finished setting parameters.");

    // 1. 预处理轨迹: 复合物完整化, 团簇化, 居中叠合, 然后生成pdb文件
    let trj = XTCTrajectory::open_read(trj).expect("Error reading trajectory");
    let frames: Vec<Rc<Frame>> = trj.into_iter().map(|p| p.unwrap()).collect();
    let total_frames = frames.len();
    // pbc whole 先不写, 先默认按照已经消除了周期性来做后续处理, 之后再看周期性的事
    let (coordinates, boxes) = get_atoms_trj(&frames);   // frames x atoms(3x1)

    // border of the whole molecule
    let min_x = coordinates.slice(s![.., .., 0]).iter().
        fold(f64::INFINITY, |prev, curr| prev.min(*curr));
    let max_x = coordinates.slice(s![.., .., 0]).iter().
        fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));
    let min_y = coordinates.slice(s![.., .., 1]).iter().
        fold(f64::INFINITY, |prev, curr| prev.min(*curr));
    let max_y = coordinates.slice(s![.., .., 1]).iter().
        fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));
    let min_z = coordinates.slice(s![.., .., 2]).iter().
        fold(f64::INFINITY, |prev, curr| prev.min(*curr));
    let max_z = coordinates.slice(s![.., .., 2]).iter().
        fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));

    let mut min_x_rec = Array1::<f64>::zeros(total_frames);
    let mut min_y_rec = Array1::<f64>::zeros(total_frames);
    let mut min_z_rec = Array1::<f64>::zeros(total_frames);
    let mut max_x_rec = Array1::<f64>::zeros(total_frames);
    let mut max_y_rec = Array1::<f64>::zeros(total_frames);
    let mut max_z_rec = Array1::<f64>::zeros(total_frames);

    let mut min_x_lig = Array1::<f64>::zeros(total_frames);
    let mut min_y_lig = Array1::<f64>::zeros(total_frames);
    let mut min_z_lig = Array1::<f64>::zeros(total_frames);
    let mut max_x_lig = Array1::<f64>::zeros(total_frames);
    let mut max_y_lig = Array1::<f64>::zeros(total_frames);
    let mut max_z_lig = Array1::<f64>::zeros(total_frames);

    let mut min_x_com = Array1::<f64>::zeros(total_frames);
    let mut min_y_com = Array1::<f64>::zeros(total_frames);
    let mut min_z_com = Array1::<f64>::zeros(total_frames);
    let mut max_x_com = Array1::<f64>::zeros(total_frames);
    let mut max_y_com = Array1::<f64>::zeros(total_frames);
    let mut max_z_com = Array1::<f64>::zeros(total_frames);

    let bf = (bt / frames[1].time as f64) as usize;
    let ef = (et / frames[1].time as f64) as usize;
    let dframe = (dt / frames[1].time as f64) as usize;
    let total_frames = (ef - bf) / dframe + 1;
    let ef = ef + 1;        // range lefts the last frame

    println!("Preparing qrv files...");
    let pb = ProgressBar::new(total_frames as u64);
    for cur_frm in (bf..ef).step_by(dframe) {
        // process each frame

        let f_name = format!("{}_{}ns", sys_name, frames[cur_frm].time / 1000.0);
        let pqr_com = wd.join(format!("{}_com.pqr", f_name));
        let mut pqr_com = File::create(pqr_com).unwrap();
        let pqr_rec = wd.join(format!("{}_rec.pqr", f_name));
        let mut pqr_rec = File::create(pqr_rec).unwrap();
        let pqr_lig = wd.join(format!("{}_lig.pqr", f_name));
        let mut pqr_lig = File::create(pqr_lig).unwrap();

        // get min and max atom coordinates
        let coordinates = coordinates.slice(s![cur_frm, .., ..]);

        let atoms_rec_x_lb: Vec<f64> = ndx_rec.iter().map(|&p| coordinates[[p, 0]] - atm_radius
            [p]).collect();
        let atoms_rec_y_lb: Vec<f64> = ndx_rec.iter().map(|&p| coordinates[[p, 1]] - atm_radius
            [p]).collect();
        let atoms_rec_z_lb: Vec<f64> = ndx_rec.iter().map(|&p| coordinates[[p, 2]] - atm_radius
            [p]).collect();
        let atoms_rec_x_ub: Vec<f64> = ndx_rec.iter().map(|&p| coordinates[[p, 0]] + atm_radius
            [p]).collect();
        let atoms_rec_y_ub: Vec<f64> = ndx_rec.iter().map(|&p| coordinates[[p, 1]] + atm_radius
            [p]).collect();
        let atoms_rec_z_ub: Vec<f64> = ndx_rec.iter().map(|&p| coordinates[[p, 2]] + atm_radius
            [p]).collect();
        min_x_rec[cur_frm] = atoms_rec_x_lb.iter().
            fold(f64::INFINITY, |prev, curr| prev.min(*curr));
        min_y_rec[cur_frm] = atoms_rec_y_lb.iter().
            fold(f64::INFINITY, |prev, curr| prev.min(*curr));
        min_z_rec[cur_frm] = atoms_rec_z_lb.iter().
            fold(f64::INFINITY, |prev, curr| prev.min(*curr));
        max_x_rec[cur_frm] = atoms_rec_x_ub.iter().
            fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));
        max_y_rec[cur_frm] = atoms_rec_y_ub.iter().
            fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));
        max_z_rec[cur_frm] = atoms_rec_z_ub.iter().
            fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));

        let atoms_lig_x_lb: Vec<f64> = ndx_lig.iter().map(|&p| coordinates[[p, 0]] - atm_radius
            [p]).collect();
        let atoms_lig_y_lb: Vec<f64> = ndx_lig.iter().map(|&p| coordinates[[p, 1]] - atm_radius
            [p]).collect();
        let atoms_lig_z_lb: Vec<f64> = ndx_lig.iter().map(|&p| coordinates[[p, 2]] - atm_radius
            [p]).collect();
        let atoms_lig_x_ub: Vec<f64> = ndx_lig.iter().map(|&p| coordinates[[p, 0]] + atm_radius
            [p]).collect();
        let atoms_lig_y_ub: Vec<f64> = ndx_lig.iter().map(|&p| coordinates[[p, 1]] + atm_radius
            [p]).collect();
        let atoms_lig_z_ub: Vec<f64> = ndx_lig.iter().map(|&p| coordinates[[p, 2]] + atm_radius
            [p]).collect();
        min_x_lig[cur_frm] = atoms_lig_x_lb.iter().
            fold(f64::INFINITY, |prev, curr| prev.min(*curr));
        min_y_lig[cur_frm] = atoms_lig_y_lb.iter().
            fold(f64::INFINITY, |prev, curr| prev.min(*curr));
        min_z_lig[cur_frm] = atoms_lig_z_lb.iter().
            fold(f64::INFINITY, |prev, curr| prev.min(*curr));
        max_x_lig[cur_frm] = atoms_lig_x_ub.iter().
            fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));
        max_y_lig[cur_frm] = atoms_lig_y_ub.iter().
            fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));
        max_z_lig[cur_frm] = atoms_lig_z_ub.iter().
            fold(f64::NEG_INFINITY, |prev, curr| prev.max(*curr));

        min_x_com[cur_frm] = min_x_rec[cur_frm].min(min_x_rec[cur_frm]);
        min_y_com[cur_frm] = min_y_rec[cur_frm].min(min_y_rec[cur_frm]);
        min_z_com[cur_frm] = min_z_rec[cur_frm].min(min_z_rec[cur_frm]);
        max_x_com[cur_frm] = max_x_rec[cur_frm].max(max_x_rec[cur_frm]);
        max_y_com[cur_frm] = max_y_rec[cur_frm].max(max_y_rec[cur_frm]);
        max_z_com[cur_frm] = max_z_rec[cur_frm].max(max_z_rec[cur_frm]);

        // loop atoms and write pqr information (from pqr)
        for &at_id in ndx_com {
            let index = atm_index[at_id];
            let at_name = &atm_name[at_id];
            let resname = &atm_resname[at_id];
            let resnum = atm_resnum[at_id];
            let coord = coordinates.slice(s![at_id, ..]);
            let x = coord[0] * 10.0;
            let y = coord[1] * 10.0;
            let z = coord[2] * 10.0;
            let q = atm_charge[at_id];
            let r = atm_radius
                [at_id];
            let atom_line = format!("ATOM  {:5} {:-4} {:3} X{:4}    {:8.3} {:8.3} {:8.3} \
            {:12.6} {:12.6}\n",
                                    index, at_name, resname, resnum, x, y, z, q, r);

            // write qrv files
            pqr_com.write_all(atom_line.as_bytes()).unwrap();
            if ndx_rec.contains(&at_id) {
                pqr_rec.write_all(atom_line.as_bytes()).unwrap();
            }
            if ndx_lig.contains(&at_id) {
                pqr_lig.write_all(atom_line.as_bytes()).unwrap();
            }
        }

        pb.inc(1);
    }

    // calculate MM and PBSA
    let Iion = Cion.dot(&(&Qion * &Qion));
    let eps0 = 8.854187812800001e-12;
    let kb = 1.380649e-23;
    let Na = 6.02214076e+23;
    let qe = 1.602176634e-19;
    let RT2kJ = 8.314462618 * temp / 1e3;
    let kap = 1e-9 / f64::sqrt(eps0 * kb * temp * sdie / (Iion * qe * qe * Na * 1e3));

    let kJcou = 1389.35457520287;
    let Rcut = f64::INFINITY;

    let total_res_num = atm_resnum[atm_resnum.len() - 1] + 1;

    let mut dE = Array1::<f64>::zeros(total_res_num);
    let mut dGres = Array1::<f64>::zeros(total_res_num);
    let mut dHres = Array1::<f64>::zeros(total_res_num);
    let mut MMres = Array1::<f64>::zeros(total_res_num);
    let mut COUres = Array1::<f64>::zeros(total_res_num);
    let mut VDWres = Array1::<f64>::zeros(total_res_num);
    let mut dPBres = Array1::<f64>::zeros(total_res_num);
    let mut dSAres = Array1::<f64>::zeros(total_res_num);

    let mut vdw = Array1::<f64>::zeros(total_frames);
    let mut pb = Array1::<f64>::zeros(total_frames);
    let mut sa = Array1::<f64>::zeros(total_frames);
    let mut cou = Array1::<f64>::zeros(total_frames);
    let mut mm = Array1::<f64>::zeros(total_frames);
    let mut dh = Array1::<f64>::zeros(total_frames);

    let mut pb_res = Array1::<f64>::zeros(total_res_num);
    let mut sa_res = Array1::<f64>::zeros(total_res_num);

    let rec_shift = ndx_rec[0];
    let lig_shift = ndx_lig[0];

    println!("Start MM/PB-SA calculations...");
    let pgb = ProgressBar::new(total_frames as u64);
    pgb.inc(0);
    let mut idx = 0;
    for cur_frm in (bf..ef).step_by(dframe) {
        // MM
        let coord = coordinates.slice(s![cur_frm, .., ..]);
        let mut de_cou = Array1::<f64>::zeros(total_res_num);
        let mut de_vdw = Array1::<f64>::zeros(total_res_num);
        // traverse receptor/ligand atoms to store parameters
        for i in 0..atom_num_rec {
            let ii = i + rec_shift;
            let qi = atm_charge[ii];
            let ci = atm_typeindex[ii];
            let xi = coord[[ii, 0]];
            let yi = coord[[ii, 1]];
            let zi = coord[[ii, 2]];
            for j in 0..atom_num_lig {
                let jj = j + lig_shift;
                let qj = atm_charge[jj];
                let cj = atm_typeindex[jj];
                let xj = coord[[jj, 0]];
                let yj = coord[[jj, 1]];
                let zj = coord[[jj, 2]];
                let r = f64::sqrt((xi - xj).powi(2) + (yi - yj).powi(2) + (zi - zj).powi(2));
                if r < Rcut {
                    let mut e_cou = qi * qj / r / 10.0;
                    if use_dh {
                        e_cou = e_cou * f64::exp(-kap * r);
                    }
                    let e_vdw = C12[[ci, cj]] / r.powi(12) - C6[[ci, cj]] / r.powi(6);
                    de_cou[atm_resnum[ii]] += e_cou;
                    de_cou[atm_resnum[jj]] += e_cou;
                    de_vdw[atm_resnum[ii]] += e_vdw;
                    de_vdw[atm_resnum[jj]] += e_vdw;
                }
            }
        }
        for i in 0..total_res_num {
            de_cou[i] *= kJcou / (2.0 * pdie);
            de_vdw[i] /= 2.0;
        }
        let e_vdw = de_vdw.sum();
        let e_cou = de_cou.sum();

        // APBS
        let f_name = format!("{}_{}ns", sys_name, frames[cur_frm].time / 1000.0);
        let mut input_apbs = File::create(wd.join(format!("{}.apbs", f_name))).unwrap();
        input_apbs.write_all("read\n".as_bytes()).expect("Failed to write apbs input file.");
        input_apbs.write_all(format!("  mol pqr {0}_com.pqr\
        \n  mol pqr {0}_rec.pqr\
        \n  mol pqr {0}_lig.pqr\
        \nend\n\n", f_name).as_bytes())
            .expect("Failed to write apbs input file.");

        if mesh_type == 0 {
            // GMXPBSA
            input_apbs.write_all(dim_apbs(format!("{}_com", f_name).as_str(), 1,
                                          min_x, max_x, min_y,
                                          max_y, min_z, max_z,
                                          cfac, fadd, df, grid_type,
                                          PBEset, PBEset0, PBAset).as_bytes()).
                expect("Failed writing apbs file.");
            input_apbs.write_all(dim_apbs(format!("{}_rec", f_name).as_str(), 2,
                                          min_x, max_x, min_y,
                                          max_y, min_z, max_z,
                                          cfac, fadd, df, grid_type,
                                          PBEset, PBEset0, PBAset).as_bytes()).
                expect("Failed writing apbs file.");
            input_apbs.write_all(dim_apbs(format!("{}_lig", f_name).as_str(), 3,
                                          min_x, max_x, min_y,
                                          max_y, min_z, max_z,
                                          cfac, fadd, df, grid_type,
                                          PBEset, PBEset0, PBAset).as_bytes()).
                expect("Failed writing apbs file.");
        } else if mesh_type == 1 {
            // g_mmpbsa
            input_apbs.write_all(dim_apbs(format!("{}_com", f_name).as_str(), 1,
                                          min_x_com[cur_frm], max_x_com[cur_frm], min_y_com[cur_frm],
                                          max_y_com[cur_frm], min_z_com[cur_frm], max_z_com[cur_frm],
                                          cfac, fadd, df, grid_type,
                                          PBEset, PBEset0, PBAset).as_bytes()).
                expect("Failed writing apbs file.");
            input_apbs.write_all(dim_apbs(format!("{}_rec", f_name).as_str(), 2,
                                          min_x_com[cur_frm], max_x_com[cur_frm], min_y_com[cur_frm],
                                          max_y_com[cur_frm], min_z_com[cur_frm], max_z_com[cur_frm],
                                          cfac, fadd, df, grid_type,
                                          PBEset, PBEset0, PBAset).as_bytes()).
                expect("Failed writing apbs file.");
            input_apbs.write_all(dim_apbs(format!("{}_lig", f_name).as_str(), 3,
                                          min_x_com[cur_frm], max_x_com[cur_frm], min_y_com[cur_frm],
                                          max_y_com[cur_frm], min_z_com[cur_frm], max_z_com[cur_frm],
                                          cfac, fadd, df, grid_type,
                                          PBEset, PBEset0, PBAset).as_bytes()).
                expect("Failed writing apbs file.");
        }

        // invoke apbs program to do apbs calculations
        let mut apbs_out = File::create(wd.join(format!("{}.out", f_name))).
            expect("Failed to create apbs out file");
        let apbs_result = Command::new(apbs).
            arg(wd.join(format!("{}.apbs", f_name))).
            current_dir(&wd).output().expect("running apbs failed.");
        let apbs_output = String::from_utf8(apbs_result.stdout).
            expect("Failed to get apbs output.");
        apbs_out.write_all(apbs_output.as_bytes()).expect("Failed to write apbs output");

        // parse output
        let apbs_info = fs::read_to_string(wd.join(format!("{}.out", f_name))).unwrap();
        let mut Esol = Array2::<f64>::zeros((3, atom_num_com));
        let mut Evac = Array2::<f64>::zeros((3, atom_num_com));
        let mut Esas = Array2::<f64>::zeros((3, atom_num_com));
        let apbs_info = apbs_info
            .split("\n")
            .filter_map(|p|
                if p.trim().starts_with("CALCULATION ") ||
                    p.trim().starts_with("Atom") ||
                    p.trim().starts_with("SASA") {
                    Some(p.trim())
                } else { None }
            )
            .collect::<Vec<&str>>()
            .join("\n");

        // extract apbs results
        let apbs_info: Vec<&str> = apbs_info        // list of apbs calculation results
            .split("CALCULATION ")
            .filter_map(|p| match p.trim().len() {
                0 => None,
                _ => Some(p.trim())
            })
            .collect();
        for info in apbs_info {
            let info: Vec<&str> = info
                .split("\n")
                .collect();

            let mut sys_idx;    // com: 0, rec: 1, lig: 2
            let mut n: f64;
            if info[0].contains(format!("{}_com", f_name).as_str()) {
                sys_idx = 0;
                n = atom_num_com as f64;
            } else if info[0].contains(format!("{}_rec", f_name).as_str()) {
                sys_idx = 1;
                n = atom_num_rec as f64;
            } else {
                sys_idx = 2;
                n = atom_num_lig as f64;
            }

            if info[0].contains("_VAC") {
                for (idx, v) in info[1..].into_iter().enumerate() {
                    let v: Vec<&str> = v
                        .split(" ")
                        .filter_map(|p| match p.trim().len() {
                            0 => None,
                            _ => Some(p)
                        }).collect();
                    let v: f64 = v[v.len() - 2].parse().unwrap();
                    Evac[[sys_idx, idx]] = v;
                }
            } else if info[0].contains("_SAS") {
                for (idx, v) in info[1..].into_iter().enumerate() {
                    let v: Vec<&str> = v
                        .split(" ")
                        .filter_map(|p| match p.trim().len() {
                            0 => None,
                            _ => Some(p)
                        }).collect();
                    let v: f64 = v[v.len() - 1].parse().unwrap();
                    Esas[[sys_idx, idx]] = gamma * v + _const / n;
                }
            } else {
                for (idx, v) in info[1..].into_iter().enumerate() {
                    let v: Vec<&str> = v
                        .split(" ")
                        .filter_map(|p| match p.trim().len() {
                            0 => None,
                            _ => Some(p)
                        }).collect();
                    let v: f64 = v[v.len() - 2].parse().unwrap();
                    Esol[[sys_idx, idx]] = v;
                }
            }
        }

        let Esol = Esol - Evac;
        let pb_com: f64 = Esol.slice(s![0, ..]).iter().sum::<f64>();
        let sa_com: f64 = Esas.slice(s![0, ..]).iter().sum::<f64>();
        let pb_rec: f64 = Esol.slice(s![1, ..]).iter().sum::<f64>();
        let sa_rec: f64 = Esas.slice(s![1, ..]).iter().sum::<f64>();
        let pb_lig: f64 = Esol.slice(s![2, ..]).iter().sum::<f64>();
        let sa_lig: f64 = Esas.slice(s![2, ..]).iter().sum::<f64>();

        vdw[idx] = e_vdw;
        cou[idx] = e_cou;
        mm[idx] = e_cou + e_vdw;
        pb[idx] = pb_com - pb_rec - pb_lig;
        sa[idx] = sa_com - sa_rec - sa_lig;
        dh[idx] = mm[idx] + pb[idx] + sa[idx];

        // residue decomposition
        for i in 0..atom_num_rec {
            dPBres[atm_resnum[i + rec_shift]] += Esol[[0, i + rec_shift]] - Esol[[1, i]];
            dSAres[atm_resnum[i + rec_shift]] += Esas[[0, i + rec_shift]] - Esas[[1, i]];
        }
        for i in 0..atom_num_lig {
            dPBres[atm_resnum[i + lig_shift]] += Esol[[0, i + lig_shift]] - Esol[[2, i]];
            dSAres[atm_resnum[i + lig_shift]] += Esas[[0, i + lig_shift]] - Esas[[2, i]];
        }

        COUres += &de_cou;
        VDWres += &de_vdw;
        pb_res += &dPBres;
        sa_res += &dSAres;

        idx += 1;
        pgb.inc(1);
    }
    pgb.finish();

    // residue time average
    COUres /= total_frames as f64;
    VDWres /= total_frames as f64;
    pb_res /= total_frames as f64;
    sa_res /= total_frames as f64;
    MMres = COUres + VDWres;
    dHres = MMres + pb_res + sa_res;

    // totally time average and ts
    let dH = dh.iter().sum::<f64>() / dh.len() as f64;
    let MM = mm.iter().sum::<f64>() / mm.len() as f64;
    let COU = cou.iter().sum::<f64>() / cou.len() as f64;
    let VDW = vdw.iter().sum::<f64>() / vdw.len() as f64;
    let PB = pb.iter().sum::<f64>() / pb.len() as f64;
    let SA = sa.iter().sum::<f64>() / sa.len() as f64;

    let TdS = mm.iter()
        .map(|&p| f64::exp((p - MM) / RT2kJ))
        .sum::<f64>() / mm.len() as f64;
    let TdS = -RT2kJ * TdS.ln();
    let dG = dH - TdS;
    let Ki = f64::exp(dG / RT2kJ);

    println!("MM-PBSA calculation finished.");
    return (dH, MM, PB, SA, COU, VDW, TdS, dG, Ki);
}

fn dim_apbs(file: &str, mol_index: i32, min_x: f64, max_x: f64, min_y: f64, max_y: f64, min_z: f64, max_z: f64,
            cfac: f64, fadd: f64, df: f64, grid_type: i32, pbe_set: &str, pbe_set0: &str, pba_set: &str) -> String {
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

    let mut c_x = 0.0;
    let mut c_y = 0.0;
    let mut c_z = 0.0;
    let mut f_x = 0.0;
    let mut f_y = 0.0;
    let mut f_z = 0.0;
    let mut n_x = 0;
    let mut n_y = 0;
    let mut n_z = 0;

    let split_level = 4;    // split level
    let t: i32 = 2_i32.pow(split_level + 1);

    match grid_type {
        0 => {
            let fpre = 1;
            let cfac = 1.7;
            f_x = x_len + 2.0 * fadd;
            c_x = f_x * cfac;
            n_x = t * ((f_x / (t as f64 * df)).round() as i32 + 1 + fpre) + 1;
            f_y = y_len + 2.0 * fadd;
            c_y = f_y * cfac;
            n_y = t * ((f_y / (t as f64 * df)).round() as i32 + 1 + fpre) + 1;
            f_z = z_len + 2.0 * fadd;
            c_z = f_z * cfac;
            n_z = t * ((f_z / (t as f64 * df)).round() as i32 + 1 + fpre) + 1;
        }
        _ => {
            c_x = x_len * cfac;
            f_x = c_x.min(x_len + fadd);
            c_y = y_len * cfac;
            f_y = c_y.min(y_len + fadd);
            c_z = z_len * cfac;
            f_z = c_z.min(z_len + fadd);
            n_x = ((f_x / df) as i32).max(t + 1);
            n_y = ((f_y / df) as i32).max(t + 1);
            n_z = ((f_z / df) as i32).max(t + 1);
        }
    }
    let mg_set = "mg-auto";
    let mem = 200 * n_x * n_y * n_z / 1024 / 1024;       // MB

    let xyz_set = format!("  {mg_set}\n  mol {mol_index}\
        \n  dime   {n_x:.0}  {n_y:.0}  {n_z:.0}        # 格点数目, 所需内存: {mem} MB\
        \n  cglen  {c_x:.3}  {c_y:.3}  {c_z:.3}        # 粗略格点长度\
        \n  fglen  {f_x:.3}  {f_y:.3}  {f_z:.3}        # 细密格点长度\
        \n  fgcent {x_center:.3}  {y_center:.3}  {z_center:.3}  # 细密格点中心\
        \n  cgcent {x_center:.3}  {y_center:.3}  {z_center:.3}  # 粗略格点中心");

    return format!("\nELEC name {file}\n\
    {xyz_set} \n\
    {pbe_set} \n\
    end\n\n\
    ELEC name {file}_VAC\n\
    {xyz_set}\n\
    {pbe_set0} \n\
    end\n\n\
    APOLAR name {file}_SAS\n  \
    mol {mol_index}\n{pba_set}\n\
    end\n\n\
    print elecEnergy {file} - {file}_VAC end\n\
    print apolEnergy {file}_SAS end\n\n");
}

pub fn gen_file_sha256<T: AsRef<Path> + ?Sized>(p: &T) -> String {
    let s = fs::read_to_string(p).unwrap();
    let hash = sha256::digest(s);
    return hash;
}
