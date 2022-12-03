use std::fmt::Debug;
use std::str::FromStr;
use std::fs;
use std::io::Write;
use std::path::Path;
use indicatif::ProgressBar;
use ndarray::{Array1, Array2};
use regex::Regex;
use crate::index_parser::Index;
use crate::mmpbsa::gen_file_sha256;
use crate::Parameters;
use crate::atom_radius::get_radi;

pub fn gen_qrv(mdp: &str, ndx: &Index, wd: &Path,
               receptor_grp: usize, ligand_grp: usize,
               qrv: &Path, settings: &Parameters) {
    // read mdp file
    let qrv = qrv.to_str().unwrap();
    let rad_type = settings.rad_type;
    let rad_lj0 = settings.rad_lj0;

    let mut qrv_content = fs::File::create(qrv).expect("Create parameter qrv file failed");
    qrv_content.write_all(format!("Receptor: {}\nLigand: {}\n",
                                  &ndx.groups[receptor_grp].name,
                                  &ndx.groups[ligand_grp].name).as_bytes())
        .expect("Writing parameter qrv file failed");
    let ndx_rec = &ndx.groups[receptor_grp].indexes;
    let ndx_lig = &ndx.groups[ligand_grp].indexes;
    let mdp_content = fs::read_to_string(mdp).unwrap();
    if mdp_content.is_empty() {
        println!("Error with {}: file empty", mdp);
    }
    let mdp_content: Vec<&str> = mdp_content.split("\n").collect();

    // get MD parameters
    let re = Regex::new(r"\s*ffparams:").unwrap();
    let locator = get_md_locators_first(&mdp_content, &re);
    // number of atom types
    let re = Regex::new(r"\s*atnr=(\d+)").unwrap();
    let atnr = re.captures(mdp_content[locator + 1]).expect("Parse mdp file error.");
    let atnr = atnr.get(1).unwrap().as_str();
    qrv_content.write_all(format!("{}\n", atnr).as_bytes()).expect("Writing parameter qrv file failed");
    let atnr: usize = atnr.parse().unwrap();
    // LJ parameters
    let locator = locator + 3;
    let mut sigma: Vec<f64> = vec![0.0; atnr];
    let mut epsilon: Vec<f64> = vec![0.0; atnr];
    let mut rad: Vec<f64> = vec![rad_lj0; atnr];

    println!("Generating qrv file...");
    println!("Writing atom L-J parameters..");
    let re = Regex::new(r".*c6\s*=\s*(.*),.*c12\s*=\s*(.*)").unwrap();
    for i in 0..atnr {
        qrv_content.write_all(format!("{:6}", i).as_bytes()).expect("Writing qrv file failed");
        // get c6 and c12 parameters for each atom
        for j in 0..atnr {
            let m = re.captures(&mdp_content[locator + i * atnr + j]).unwrap();
            let c6 = m.get(1).unwrap().as_str();
            let c12 = m.get(2).unwrap().as_str().trim();
            qrv_content.write_all(format!(" {} {}", c6, c12).as_bytes()).expect("Writing qrv file failed");
            let c6: f64 = c6.parse().unwrap();
            let c12: f64 = c12.parse().unwrap();
            // calculate σ, ε, radius for each atom
            if j == i && c6 != 0.0 && c12 != 0.0 {
                sigma[i] = 10.0 * (c12 / c6).powf(1.0 / 6.0); // 转换单位为A
                epsilon[i] = c6.powi(2) / (4.0 * c12);
                rad[i] = 0.5 * sigma[i]; // sigma为直径
            }
        }
        qrv_content.write_all("\n".as_bytes()).expect("Writing qrv file failed");
    }

    // number of molecule types
    let re = Regex::new(r"#molblock\s*=\s*(.+?)\s*").unwrap();
    let mol_types: usize = get_md_params_first(&mdp_content, &re).parse().unwrap();
    // number of molecules of each type
    let re = Regex::new(r"moltype\s*=\s*.+").unwrap();
    let mol_type_locators: Vec<usize> = get_md_locators_all(&mdp_content, &re);
    let mut mol_nums: Array1<usize> = Array1::zeros(mol_type_locators.len());
    let re = Regex::new(r"#molecules.*=\s*(\d+)\s*").unwrap();
    for (i_mol, loc) in mol_type_locators.into_iter().enumerate() {
        let s = mdp_content[loc + 1];
        match re.captures(s).unwrap().get(1) {
            Some(i) => {
                mol_nums[i_mol] = i.as_str().parse().unwrap();
            },
            _ => ()
        }
    }
    // number of atoms
    let re = Regex::new(r"atom \((.+)\):").unwrap();
    let max_atm_num: usize = *get_md_params_all(&mdp_content, &re).iter().max().unwrap();
    let mut sys_names: Vec<String> = vec![];                // name of each system
    let mut sys_atom_nums: Vec<usize> = vec![];             // atom number of each system

    // locator of molecule types
    let re = Regex::new(r"\s*moltype.+\(").unwrap();
    let locators = get_md_locators_all(&mdp_content, &re);

    // initialize atom information, each molecule per line
    // 考虑使用泛型值来优化
    let mut res_ids: Array2<usize> = Array2::zeros((mol_nums.len(), max_atm_num));  // residue ids of each atom
    let mut c_atoms: Array2<usize> = Array2::zeros((mol_nums.len(), max_atm_num));  // atom type
    let mut r_atoms: Array2<f64> = Array2::zeros((mol_nums.len(), max_atm_num));    // atom radius
    let mut s_atoms: Array2<f64> = Array2::zeros((mol_nums.len(), max_atm_num));    // atom sigma
    let mut e_atoms: Array2<f64> = Array2::zeros((mol_nums.len(), max_atm_num));    // atom epsilon
    let mut q_atoms: Array2<f64> = Array2::zeros((mol_nums.len(), max_atm_num));    // atom charge
    let mut t_atoms: Array2<String> = Array2::default((mol_nums.len(), max_atm_num));   // atom name

    // get atom parameters
    for locator in locators {
        let re = Regex::new(r"\s*moltype.+\((\d+)\)").unwrap();
        let mol_id = re.captures(&mdp_content[locator]).unwrap().get(1).unwrap();
        let mol_id: usize = mol_id.as_str().parse().unwrap();
        let re = Regex::new(r"name=(.*)").unwrap();
        let n = re.captures(&mdp_content[locator + 1]).unwrap().get(1).unwrap().as_str().trim();
        sys_names.push(n[1..(n.len() - 1)].to_string());
        let re = Regex::new(r"\((.*)\)").unwrap();
        let num = re.captures(&mdp_content[locator + 3]).unwrap().get(1).unwrap();
        let num: usize = num.as_str().parse().unwrap();
        sys_atom_nums.push(num);
        let locator = locator + 4;

        println!("Reading the {}/{} molecule's information.", mol_id + 1, mol_nums.len());
        println!("Reading atom property parameters...");
        let re = Regex::new(r".*type=\s*(\d+).*q=\s*([^,]+),.*resind=\s*(\d+).*").unwrap();
        for i in 0..sys_atom_nums[mol_id] {
            let c = re.captures(&mdp_content[locator + i]).unwrap();
            let at_type = c.get(1).unwrap();
            let at_type: usize = at_type.as_str().parse().unwrap();
            let res_id = c.get(3).unwrap();
            let res_id: usize = res_id.as_str().parse().unwrap();
            res_ids[[mol_id, i]] = res_id;
            c_atoms[[mol_id, i]] = at_type;
            r_atoms[[mol_id, i]] = rad[at_type];
            s_atoms[[mol_id, i]] = sigma[at_type];
            e_atoms[[mol_id, i]] = epsilon[at_type];
            let q = c.get(2).unwrap();
            let q: f64 = q.as_str().parse().unwrap();
            q_atoms[[mol_id, i]] = q;
        }
        let locator = locator + sys_atom_nums[mol_id] + 1;     // 不加1是"atom (3218):"行

        // get atom names
        println!("Reading atom names...");
        let re = Regex::new("name=\"(.*)\"").unwrap();
        for i in 0..sys_atom_nums[mol_id] {
            let name = re.captures(&mdp_content[locator + i]).unwrap();
            let name = name.get(1).unwrap().as_str();
            t_atoms[[mol_id, i]] = name.to_string();
        }
    }

    // get residues information
    let re = Regex::new(r"\s*residue \((\d+)\)").unwrap();
    let locators = get_md_locators_all(&mdp_content, &re);
    let mut resnums: Vec<usize> = vec![];
    for i in 0..locators.len() {
        let res_num = re.captures(&mdp_content[locators[i]]).unwrap().get(1).unwrap();
        let res_num: usize = res_num.as_str().trim().parse().unwrap();
        resnums.push(res_num);
    }
    let max_res_num: usize = *resnums.iter().max().unwrap() as usize;
    let mut res_names = Array2::<String>::default((mol_nums.len(), max_res_num));

    let re = Regex::new(".*name=\"(.+)\",.*nr=(\\d+).*").unwrap();
    for (idx, locator) in locators.into_iter().enumerate() {
        println!("Reading residues information...");
        for i in 0..resnums[idx] {
            let m = re.captures(&mdp_content[locator + 1 + i]).unwrap();
            let name = m.get(1).unwrap().as_str();
            let nr = m.get(2).unwrap().as_str();
            let nr: i32 = nr.parse().unwrap();
            res_names[[idx, i]] = format!("{:05}{}", nr, name);
        }
    }

    // assign H types by connection atoms from angle information
    let re = Regex::new(r"^ +Angle:").unwrap();
    let locators = get_md_locators_all(&mdp_content, &re);
    println!("Reading angles...");
    let re = Regex::new(r"\d+ type=\d+ \(ANGLES\)\s+(\d+)\s+(\d+)\s+(\d+)").unwrap();
    for (mol_id, locator) in locators.into_iter().enumerate() {
        let angles_num: Vec<&str> = (&mdp_content[locator + 1]).trim().split(" ").collect();
        let angles_num: usize = angles_num[1].parse().unwrap();
        let angles_num = angles_num / 4;
        if angles_num > 0 {
            for l_num in locator + 3..locator + 3 + angles_num {
                if re.is_match(&mdp_content[l_num]) {
                    let paras = re.captures(&mdp_content[l_num]).unwrap();
                    let i = paras.get(1).unwrap();
                    let i: usize = i.as_str().parse().unwrap();
                    let j = paras.get(2).unwrap();
                    let j: usize = j.as_str().parse().unwrap();
                    let k = paras.get(3).unwrap();
                    let k: usize = k.as_str().trim().parse().unwrap();
                    if t_atoms[[mol_id, i]].starts_with(['H', 'h']) {
                        t_atoms[[mol_id, i]] = format!("H{}", t_atoms[[mol_id, j]]);
                    }
                    if t_atoms[[mol_id, k]].starts_with(['H', 'h']) {
                        t_atoms[[mol_id, k]] = format!("H{}", t_atoms[[mol_id, j]]);
                    }
                } else { break; }
            }
        }
    }

    // output to qrv file
    let mut atom_id_total = 0;
    let mut atom_id_feature = 0;
    let re = Regex::new(r"([a-zA-Z]+)\d*").unwrap();
    for i in 0..mol_types {
        for n in 0..mol_nums[i] {
            println!("Writing atoms...");
            for j in 0..sys_atom_nums[i] {
                if ndx_rec.contains(&atom_id_total) || ndx_lig.contains(&atom_id_total) {
                    atom_id_feature += 1;
                    let mut radi: f64;
                    match rad_type {
                        0 => radi = r_atoms[[n, j]],
                        1 => radi = {
                            let res = re.captures(t_atoms[[i, j]].as_str()).unwrap();
                            let res = res.get(1).unwrap().as_str();
                            get_radi(res)
                        },
                        _ => {
                            println!("Error: radType should only be 0 or 1. Check settings.");
                            return;
                        }
                    }
                    qrv_content.write_all(format!("{:6} {:9.5} {:9.6} {:6} {:9.6} {:9.6} {:6} \"{}\"-1.{} {} {:-6}  ",
                                                  atom_id_feature, q_atoms[[i, j]], radi, c_atoms[[i, j]], s_atoms[[i, j]],
                                                  e_atoms[[i, j]], atom_id_total + 1, sys_names[i], j + 1,
                                                  res_names[[i, res_ids[[i, j]]]], t_atoms[[i, j]]).as_bytes())
                        .expect("Writing qrv file failed.");
                    if ndx_rec.contains(&atom_id_total) {
                        qrv_content.write_all("Rec\n".as_bytes()).expect("Writing qrv file failed.");
                    } else if ndx_lig.contains(&atom_id_total) {
                        qrv_content.write_all("Lig\n".as_bytes()).expect("Writing qrv file failed.");
                    }
                }
                atom_id_total += 1;
            }
        }
    }

    // generate md5 for tpr file
    if Path::new(mdp).is_file() {
        println!("Generating mdp sha...");
        let mut mdp_sha = fs::File::create(wd.join(".mdp.sha")).unwrap();
        mdp_sha.write_all(gen_file_sha256(mdp).as_bytes()).expect("Failed to write sha");
    }
    if Path::new(qrv).is_file() {
        println!("Generating qrv sha...");
        let mut qrv_sha = fs::File::create(wd.join(".qrv.sha")).unwrap();
        qrv_sha.write_all(gen_file_sha256(qrv).as_bytes()).expect("Failed to write sha");
    }

    println!("Finished generating qrv file.");
}

fn get_md_locators_first(strings: &Vec<&str>, re: &Regex) -> usize {
    for (idx, l) in strings.into_iter().enumerate() {
        if re.is_match(l) {
            return idx;
        }
    }
    return 0;
}

fn get_md_locators_all(strings: &Vec<&str>, re: &Regex) -> Vec<usize> {
    let mut locators: Vec<usize> = vec![];
    for (idx, l) in strings.into_iter().enumerate() {
        if re.is_match(l) {
            locators.push(idx);
        }
    }
    return locators;
}

#[warn(dead_code)]
fn get_md_params_first(strings: &Vec<&str>, re: &Regex) -> String {
    for line in strings {
        if re.is_match(line) {
            let value = re.captures(line).unwrap();
            let value = value.get(1).unwrap().as_str();
            return value.to_string();
        }
    }
    return String::new();
}

fn get_md_params_all<T: Debug + FromStr>(strings: &Vec<&str>, re: &Regex) -> Vec<T> where <T as FromStr>::Err: Debug {
    let mut values: Vec<T> = vec![];
    for line in strings {
        if re.is_match(line) {
            let value = re.captures(line).unwrap();
            let value = value.get(1).unwrap().as_str();
            let value: T = value.trim().parse().unwrap();
            values.push(value);
        }
    }
    return values;
}
