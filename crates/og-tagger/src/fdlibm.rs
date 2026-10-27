fn hi_word(x: f64) -> i32 {
    (x.to_bits() >> 32) as u32 as i32
}

fn lo_word(x: f64) -> i32 {
    x.to_bits() as u32 as i32
}

fn with_hi(x: f64, hi: i32) -> f64 {
    f64::from_bits((x.to_bits() & 0x0000_FFFF_FFFF) | ((hi as u32 as u64) << 32))
}

pub fn exp(x: f64) -> f64 {
    const HUGE: f64 = 1.0e300;
    const TWOM1000: f64 = f64::from_bits(0x0170_0000_0000_0000);
    const O_THRESHOLD: f64 = f64::from_bits(0x4086_2E42_FEFA_39EF);
    const U_THRESHOLD: f64 = f64::from_bits(0xC087_4910_D52D_3051);
    const LN2_HI: f64 = f64::from_bits(0x3FE6_2E42_FEE0_0000);
    const LN2_LO: [f64; 2] = [
        f64::from_bits(0x3DEA_39EF_3579_3C76),
        f64::from_bits(0xBDEA_39EF_3579_3C76),
    ];
    const INV_LN2: f64 = f64::from_bits(0x3FF7_1547_652B_82FE);
    const P1: f64 = f64::from_bits(0x3FC5_5555_5555_553E);
    const P2: f64 = f64::from_bits(0xBF66_C16C_16BE_BD93);
    const P3: f64 = f64::from_bits(0x3F11_566A_AF25_DE2C);
    const P4: f64 = f64::from_bits(0xBEBB_BD41_C5D2_6BF1);
    const P5: f64 = f64::from_bits(0x3E66_3769_72BE_A4D0);

    let mut x = x;
    let mut hi = 0.0;
    let mut lo = 0.0;
    let mut k = 0i32;

    let mut hx = (x.to_bits() >> 32) as u32;
    let xsb = ((hx >> 31) & 1) as i32;
    hx &= 0x7FFF_FFFF;

    if hx >= 0x4086_2E42 {
        if hx >= 0x7FF0_0000 {
            if ((hx & 0xF_FFFF) | x.to_bits() as u32) != 0 {
                return x + x;
            }
            return if xsb == 0 { x } else { 0.0 };
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE;
        }
        if x < U_THRESHOLD {
            return TWOM1000 * TWOM1000;
        }
    }

    if hx > 0x3FD6_2E42 {
        if hx < 0x3FF0_A2B2 {
            hi = x - LN2_HI * (1 - 2 * xsb) as f64;
            lo = LN2_LO[xsb as usize];
            k = 1 - xsb - xsb;
        } else {
            k = (INV_LN2 * x + 0.5 * (1 - 2 * xsb) as f64) as i32;
            let t = k as f64;
            hi = x - t * LN2_HI;
            lo = t * LN2_LO[0];
        }
        x = hi - lo;
    } else if hx < 0x3E30_0000 {
        if HUGE + x > 1.0 {
            return 1.0 + x;
        }
    } else {
        k = 0;
    }

    let t = x * x;
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);

    if k >= -1021 {
        with_hi(y, hi_word(y).wrapping_add(k << 20))
    } else {
        with_hi(y, hi_word(y).wrapping_add((k + 1000) << 20)) * TWOM1000
    }
}

pub fn log(x: f64) -> f64 {
    const TWO54: f64 = f64::from_bits(0x4350_0000_0000_0000);
    const LN2_HI: f64 = f64::from_bits(0x3FE6_2E42_FEE0_0000);
    const LN2_LO: f64 = f64::from_bits(0x3DEA_39EF_3579_3C76);
    const LG1: f64 = f64::from_bits(0x3FE5_5555_5555_5593);
    const LG2: f64 = f64::from_bits(0x3FD9_9999_9997_FA04);
    const LG3: f64 = f64::from_bits(0x3FD2_4924_9422_9359);
    const LG4: f64 = f64::from_bits(0x3FCC_71C5_1D8E_78AF);
    const LG5: f64 = f64::from_bits(0x3FC7_4664_96CB_03DE);
    const LG6: f64 = f64::from_bits(0x3FC3_9A09_D078_C69F);
    const LG7: f64 = f64::from_bits(0x3FC2_F112_DF3E_5244);

    let mut x = x;
    let mut k = 0i32;

    let mut hx = hi_word(x);
    let lx = lo_word(x);

    if hx < 0x0010_0000 {
        if (((hx as u32) & 0x7FFF_FFFF) | lx as u32) == 0 {
            return -TWO54 / 0.0;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = hi_word(x);
    }
    if hx >= 0x7FF0_0000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000F_FFFF;
    let i = (hx + 0x9_5F64) & 0x10_0000;
    x = with_hi(x, hx | (i ^ 0x3FF0_0000));
    k += i >> 20;
    let f = x - 1.0;
    if (0x000F_FFFF & (2 + hx)) < 3 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = k as f64;
            return dk * LN2_HI + dk * LN2_LO;
        }
        let r = f * f * (0.5 - 0.333_333_333_333_333_3 * f);
        let dk = k as f64;
        if k == 0 {
            return f - r;
        }
        return dk * LN2_HI - ((r - dk * LN2_LO) - f);
    }
    let s = f / (2.0 + f);
    let dk = k as f64;
    let z = s * s;
    let i = hx - 0x6_147A;
    let w = z * z;
    let j = 0x6B851 - hx;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    let ij = i | j;
    let r = t2 + t1;
    if ij > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            return f - (hfsq - s * (hfsq + r));
        }
        dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

#[cfg(test)]
fn ulp_distance(a: f64, b: f64) -> u64 {
    if a == b {
        return 0;
    }
    if a.is_sign_negative() != b.is_sign_negative() {
        return u64::MAX;
    }
    a.abs().to_bits().abs_diff(b.abs().to_bits())
}

#[cfg(test)]
struct Lcg(u64);

#[cfg(test)]
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
}

#[test]
fn exp_special_cases_match_fdlibm_spec() {
    let vectors: &[(u64, u64)] = &[
        (0x0000_0000_0000_0000, 0x3FF0_0000_0000_0000),
        (0x8000_0000_0000_0000, 0x3FF0_0000_0000_0000),
        (0x3FF0_0000_0000_0000, 0x4005_BF0A_8B14_576A),
        (0xBFF0_0000_0000_0000, 0x3FD7_8B56_362C_EF38),
        (0x4000_0000_0000_0000, 0x401D_8E64_B8D4_DDAE),
        (0x3FE0_0000_0000_0000, 0x3FFA_6129_8E1E_069C),
        (0x4086_2E3D_70A3_D70A, 0x7FEF_E9CE_5C4C_52B4),
        (0xC087_48CC_CCCC_CCCD, 0x0000_0000_0000_0001),
        (0x4086_3000_0000_0000, 0x7FF0_0000_0000_0000),
        (0xC087_5000_0000_0000, 0x0000_0000_0000_0000),
        (0x01A5_6E1F_C2F8_F359, 0x3FF0_0000_0000_0000),
        (0x81A5_6E1F_C2F8_F359, 0x3FF0_0000_0000_0000),
        (0x3FEF_FFFF_FFF2_4190, 0x4005_BF0A_8B0B_0063),
        (0x3FF0_0000_0006_DF38, 0x4005_BF0A_8B1D_AE70),
        (0x4062_D534_D5F7_E110, 0x4D84_8A1D_294F_35AC),
        (0x4083_AC24_51CB_0B52, 0x78B2_67F7_1ADF_B57E),
    ];
    for (x, expected) in vectors {
        assert_eq!(exp(f64::from_bits(*x)).to_bits(), *expected);
    }
    assert!(exp(f64::INFINITY).is_infinite() && exp(f64::INFINITY) > 0.0);
    assert_eq!(exp(f64::NEG_INFINITY), 0.0);
    assert!(exp(f64::NAN).is_nan());
    assert!(exp(710.0).is_infinite());
    assert_eq!(exp(-746.0), 0.0);
}

#[test]
fn log_special_cases_match_fdlibm_spec() {
    let vectors: &[(u64, u64)] = &[
        (0x3FF0_0000_0000_0000, 0x0000_0000_0000_0000),
        (0x4000_0000_0000_0000, 0x3FE6_2E42_FEFA_39EF),
        (0x3FE0_0000_0000_0000, 0xBFE6_2E42_FEFA_39EF),
        (0x4086_2E3D_70A3_D70A, 0x401A_4283_947A_D848),
        (0x01A5_6E1F_C2F8_F359, 0xC085_9634_47F8_7FB5),
        (0x3FEF_FFFF_FFF2_4190, 0xBDDB_7CE0_0005_E728),
        (0x3FF0_0000_0006_DF38, 0x3DDB_7CDF_FFFA_18D8),
        (0x3162_B5AE_1DB8_B2D9, 0xC064_2B19_50E1_D348),
        (0x31E1_FB2A_FED8_F087, 0xC063_7AEC_9972_0733),
    ];
    for (x, expected) in vectors {
        assert_eq!(log(f64::from_bits(*x)).to_bits(), *expected);
    }
    assert!(log(0.0).is_infinite() && log(0.0).is_sign_negative());
    assert!(log(f64::NEG_INFINITY).is_nan());
    assert!(log(-1.0).is_nan());
    assert!(log(f64::INFINITY).is_infinite());
    assert!(log(f64::NAN).is_nan());
}

#[test]
fn exp_within_one_ulp_of_correctly_rounded() {
    let mut lcg = Lcg(0x853C49E6748FEA9B);
    for _ in 0..200_000 {
        let r = lcg.next();
        let x = ((r >> 11) as i64 as f64 / (1u64 << 53) as f64) * 745.0;
        let d = ulp_distance(exp(x), x.exp());
        assert!(d <= 1, "exp({x}) diverged by {d} ulps");
    }
    for i in -300_000i64..300_000 {
        let x = i as f64 * 0.002;
        let d = ulp_distance(exp(x), x.exp());
        assert!(d <= 1, "exp({x}) diverged by {d} ulps");
    }
    for e in -1074i32..1024 {
        let x = f64::from_bits((((1023 + e) as u64) << 52) | (lcg.next() >> 12));
        if x.is_finite() && x <= 745.0 {
            assert!(ulp_distance(exp(x), x.exp()) <= 1, "exp({x}) diverged");
        }
        let xn = -x;
        if xn >= -745.0 {
            assert!(ulp_distance(exp(xn), xn.exp()) <= 1, "exp({xn}) diverged");
        }
    }
}

#[test]
fn log_within_one_ulp_of_correctly_rounded() {
    let mut lcg = Lcg(0x243F6A8885A308D3);
    for _ in 0..200_000 {
        let r = lcg.next();
        let e = (r >> 11) % 1024;
        let x = f64::from_bits((e << 52) | (r & 0x000F_FFFF_FFFF_FFFF));
        if !x.is_finite() || x == 0.0 {
            continue;
        }
        let d = ulp_distance(log(x), x.ln());
        assert!(d <= 1, "log({x}) diverged by {d} ulps");
    }
    for i in 1..300_000u64 {
        let x = i as f64 * 0.002;
        let d = ulp_distance(log(x), x.ln());
        assert!(d <= 1, "log({x}) diverged by {d} ulps");
    }
    for e in 0..1024u64 {
        let x = f64::from_bits((e << 52) | (lcg.next() & 0x000F_FFFF_FFFF_FFFF));
        if x.is_finite() && x > 0.0 {
            assert!(ulp_distance(log(x), x.ln()) <= 1, "log({x}) diverged");
        }
    }
    let sub = f64::from_bits(lcg.next() >> 12);
    if sub > 0.0 {
        assert!(ulp_distance(log(sub), sub.ln()) <= 1);
    }
}
