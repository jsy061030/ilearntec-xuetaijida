//! DES 加密模块
//!
//! 完整移植自 iLearnPy/des.py —— 吉林大学 CAS 统一认证前端 DES 加密。
//! 与 SSO 的 `des.js` 输出完全一致。
//!
//! 加密流程：`str_enc(username + password + lt, "1", "2", "3")`
//!   - 每 4 字符一个分组，64 bit 块加密
//!   - 三密钥迭代 (Triple DES 变种)
//!   - 输出十六进制大写字符串

/// 将字符串转换为 64 位二进制数组 (每元素 0 或 1)
/// 取字符串前 4 个字符，每个字符展开为 16 bit 二进制
fn str_to_bt(s: &str) -> Vec<u8> {
    let mut bt = vec![0u8; 64];
    let chars: Vec<char> = s.chars().take(4).collect();
    let n = chars.len();

    for i in 0..n {
        let k = chars[i] as u32;
        for j in 0..16 {
            let pow = 1u32 << (15 - j);
            bt[16 * i + j] = ((k / pow) % 2) as u8;
        }
    }
    // n < 4 的位置已在初始化时置零，无需额外处理

    bt
}

/// 初始置换 (IP)
fn init_permute(data: &[u8]) -> Vec<u8> {
    let mut ip = vec![0u8; 64];
    let pairs: [(usize, usize); 4] = [(1, 0), (3, 2), (5, 4), (7, 6)];

    for i in 0..4 {
        for j in (0..8).rev() {
            let k = 7 - j;
            ip[i * 8 + k] = data[j * 8 + pairs[i].0];
            ip[i * 8 + k + 32] = data[j * 8 + pairs[i].1];
        }
    }
    ip
}

/// 扩展置换 (E) — 32 bit → 48 bit
fn expand_permute(right: &[u8]) -> Vec<u8> {
    let mut ep = vec![0u8; 48];
    for i in 0..8 {
        ep[i * 6] = if i == 0 { right[31] } else { right[i * 4 - 1] };
        ep[i * 6 + 1] = right[i * 4];
        ep[i * 6 + 2] = right[i * 4 + 1];
        ep[i * 6 + 3] = right[i * 4 + 2];
        ep[i * 6 + 4] = right[i * 4 + 3];
        ep[i * 6 + 5] = if i == 7 { right[0] } else { right[i * 4 + 4] };
    }
    ep
}

/// 逐位异或
fn xor(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect()
}

/// S盒置换 — 48 bit → 32 bit
fn sbox_permute(expand: &[u8]) -> Vec<u8> {
    let sboxes: [[[u8; 16]; 4]; 8] = [
        [
            [14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7],
            [0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12, 11, 9, 5, 3, 8],
            [4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0],
            [15, 12, 8, 2, 4, 9, 1, 7, 5, 11, 3, 14, 10, 0, 6, 13],
        ],
        [
            [15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10],
            [3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1, 10, 6, 9, 11, 5],
            [0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15],
            [13, 8, 10, 1, 3, 15, 4, 2, 11, 6, 7, 12, 0, 5, 14, 9],
        ],
        [
            [10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8],
            [13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14, 12, 11, 15, 1],
            [13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7],
            [1, 10, 13, 0, 6, 9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12],
        ],
        [
            [7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15],
            [13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12, 1, 10, 14, 9],
            [10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4],
            [3, 15, 0, 6, 10, 1, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14],
        ],
        [
            [2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9],
            [14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15, 10, 3, 9, 8, 6],
            [4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14],
            [11, 8, 12, 7, 1, 14, 2, 13, 6, 15, 0, 9, 10, 4, 5, 3],
        ],
        [
            [12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11],
            [10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13, 14, 0, 11, 3, 8],
            [9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6],
            [4, 3, 2, 12, 9, 5, 15, 10, 11, 14, 1, 7, 6, 0, 8, 13],
        ],
        [
            [4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1],
            [13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5, 12, 2, 15, 8, 6],
            [1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2],
            [6, 11, 13, 8, 1, 4, 10, 7, 9, 5, 0, 15, 14, 2, 3, 12],
        ],
        [
            [13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7],
            [1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6, 11, 0, 14, 9, 2],
            [7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8],
            [2, 1, 14, 7, 4, 10, 8, 13, 15, 12, 9, 0, 3, 5, 6, 11],
        ],
    ];

    let mut sbox_result = vec![0u8; 32];

    for m in 0..8 {
        let i = (expand[m * 6] as usize * 2) + expand[m * 6 + 5] as usize;
        let j = (expand[m * 6 + 1] as usize * 8)
            + (expand[m * 6 + 2] as usize * 4)
            + (expand[m * 6 + 3] as usize * 2)
            + expand[m * 6 + 4] as usize;

        let val = sboxes[m][i][j];
        // 转换为 4 位二进制
        sbox_result[m * 4] = (val >> 3) & 1;
        sbox_result[m * 4 + 1] = (val >> 2) & 1;
        sbox_result[m * 4 + 2] = (val >> 1) & 1;
        sbox_result[m * 4 + 3] = val & 1;
    }

    sbox_result
}

/// P盒置换 — 32 bit → 32 bit
fn p_permute(sbox: &[u8]) -> Vec<u8> {
    vec![
        sbox[15], sbox[6], sbox[19], sbox[20], sbox[28], sbox[11], sbox[27], sbox[16],
        sbox[0], sbox[14], sbox[22], sbox[25], sbox[4], sbox[17], sbox[30], sbox[9],
        sbox[1], sbox[7], sbox[23], sbox[13], sbox[31], sbox[26], sbox[2], sbox[8],
        sbox[18], sbox[12], sbox[29], sbox[5], sbox[21], sbox[10], sbox[3], sbox[24],
    ]
}

/// 最终置换 (FP) — 64 bit → 64 bit
fn finally_permute(end: &[u8]) -> Vec<u8> {
    vec![
        end[39], end[7], end[47], end[15], end[55], end[23], end[63], end[31],
        end[38], end[6], end[46], end[14], end[54], end[22], end[62], end[30],
        end[37], end[5], end[45], end[13], end[53], end[21], end[61], end[29],
        end[36], end[4], end[44], end[12], end[52], end[20], end[60], end[28],
        end[35], end[3], end[43], end[11], end[51], end[19], end[59], end[27],
        end[34], end[2], end[42], end[10], end[50], end[18], end[58], end[26],
        end[33], end[1], end[41], end[9], end[49], end[17], end[57], end[25],
        end[32], end[0], end[40], end[8], end[48], end[16], end[56], end[24],
    ]
}

/// 生成 16 轮子密钥 — 64 bit → 16×48 bit
fn generate_keys(key_byte: &[u8]) -> [Vec<u8>; 16] {
    let loop_shifts = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];
    let mut key = vec![0u8; 56];

    // PC-1: 64→56
    for i in 0..7 {
        for j in 0..8 {
            let k = 7 - j;
            key[i * 8 + j] = key_byte[8 * k + i];
        }
    }

    let mut keys: [Vec<u8>; 16] = Default::default();

    for round in 0..16 {
        // 左移
        for _ in 0..loop_shifts[round] {
            let temp0 = key[0];
            let temp28 = key[28];
            for k in 0..27 {
                key[k] = key[k + 1];
                key[28 + k] = key[29 + k];
            }
            key[27] = temp0;
            key[55] = temp28;
        }

        // PC-2: 56→48
        let pc2 = [
            13, 16, 10, 23, 0, 4, 2, 27, 14, 5, 20, 9, 22, 18, 11, 3,
            25, 7, 15, 6, 26, 19, 12, 1, 40, 51, 30, 36, 46, 54, 29, 39,
            50, 44, 32, 47, 43, 48, 38, 55, 33, 52, 45, 41, 49, 35, 28, 31,
        ];
        keys[round] = pc2.iter().map(|&idx| key[idx]).collect();
    }

    keys
}

/// 单次 DES 加密 — 64 bit 数据 + 64 bit 密钥 → 64 bit 密文
fn enc(data_byte: &[u8], key_byte: &[u8]) -> Vec<u8> {
    let keys = generate_keys(key_byte);
    let ip_byte = init_permute(data_byte);

    let mut ip_left: Vec<u8> = ip_byte[..32].to_vec();
    let mut ip_right: Vec<u8> = ip_byte[32..].to_vec();

    for round in 0..16 {
        let temp_left = ip_left.clone();
        ip_left = ip_right.clone();

        let expanded = expand_permute(&ip_right);
        let xored = xor(&expanded, &keys[round]);
        let sboxed = sbox_permute(&xored);
        let permuted = p_permute(&sboxed);
        let temp_right = xor(&permuted, &temp_left);

        ip_right = temp_right;
    }

    // 交换左右半
    let mut final_data = vec![0u8; 64];
    final_data[..32].copy_from_slice(&ip_right);
    final_data[32..].copy_from_slice(&ip_left);

    finally_permute(&final_data)
}

/// 三密钥迭代加密
fn encrypt_with_keys(
    bt: &[u8],
    keys1: Option<&[Vec<u8>]>,
    keys2: Option<&[Vec<u8>]>,
    keys3: Option<&[Vec<u8>]>,
) -> Vec<u8> {
    let mut temp = bt.to_vec();

    if let Some(k1) = keys1 {
        for key in k1 {
            temp = enc(&temp, key);
        }
    }
    if let Some(k2) = keys2 {
        for key in k2 {
            temp = enc(&temp, key);
        }
    }
    if let Some(k3) = keys3 {
        for key in k3 {
            temp = enc(&temp, key);
        }
    }

    temp
}

/// 获取密钥字节数组（多个 64 bit 密钥块）
fn get_key_bytes(key: &str) -> Vec<Vec<u8>> {
    let chars: Vec<char> = key.chars().collect();
    let iterator = chars.len() / 4;
    let mut key_bytes = Vec::with_capacity(iterator + 1);

    for i in 0..iterator {
        let chunk: String = chars[i * 4..i * 4 + 4].iter().collect();
        key_bytes.push(str_to_bt(&chunk));
    }

    if chars.len() % 4 > 0 {
        let remainder: String = chars[iterator * 4..].iter().collect();
        key_bytes.push(str_to_bt(&remainder));
    }

    key_bytes
}

/// 将 64 bit 二进制数组转换为十六进制字符串
fn bt64_to_hex(byte_data: &[u8]) -> String {
    let mut hex_str = String::with_capacity(16);
    for i in 0..16 {
        let mut val: u8 = 0;
        for j in 0..4 {
            val = (val << 1) | byte_data[i * 4 + j];
        }
        hex_str.push_str(&format!("{:X}", val));
    }
    hex_str
}

/// 字符串 DES 加密 (Triple DES 变种)
///
/// 输入明文按 4 字符分组，每组依次用三把密钥的所有子密钥加密。
/// 输出大写十六进制字符串。
///
/// # Examples
///
/// ```
/// let result = ilearn_core::des::str_enc("ABCD", "1", "2", "3");
/// assert_eq!(result, "50D0FE100D95D3F8");
/// ```
pub fn str_enc(data: &str, first_key: &str, second_key: &str, third_key: &str) -> String {
    let mut enc_data = String::new();

    let keys1: Option<Vec<Vec<u8>>> = if first_key.is_empty() {
        None
    } else {
        Some(get_key_bytes(first_key))
    };
    let keys2: Option<Vec<Vec<u8>>> = if second_key.is_empty() {
        None
    } else {
        Some(get_key_bytes(second_key))
    };
    let keys3: Option<Vec<Vec<u8>>> = if third_key.is_empty() {
        None
    } else {
        Some(get_key_bytes(third_key))
    };

    if data.is_empty() {
        return enc_data;
    }

    let chars: Vec<char> = data.chars().collect();
    let iterator = chars.len() / 4;
    let remainder = chars.len() % 4;

    for i in 0..iterator {
        let chunk: String = chars[i * 4..i * 4 + 4].iter().collect();
        let bt = str_to_bt(&chunk);
        let enc_byte = encrypt_with_keys(
            &bt,
            keys1.as_deref(),
            keys2.as_deref(),
            keys3.as_deref(),
        );
        enc_data.push_str(&bt64_to_hex(&enc_byte));
    }

    if remainder > 0 {
        let chunk: String = chars[iterator * 4..].iter().collect();
        let bt = str_to_bt(&chunk);
        let enc_byte = encrypt_with_keys(
            &bt,
            keys1.as_deref(),
            keys2.as_deref(),
            keys3.as_deref(),
        );
        enc_data.push_str(&bt64_to_hex(&enc_byte));
    }

    enc_data
}

/// 便捷函数：与 SSO 前端 `desInit()` 完全一致
///
/// ```
/// let cipher = ilearn_core::des::des_init("testuser", "testpass", "LT-TEST");
/// // cipher 可直接填入 SSO 登录表单的 rsa 字段
/// ```
pub fn des_init(username: &str, password: &str, lt: &str) -> String {
    let data = format!("{}{}{}", username, password, lt);
    str_enc(&data, "1", "2", "3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_str_enc_abcd() {
        let result = str_enc("ABCD", "1", "2", "3");
        assert_eq!(result, "50D0FE100D95D3F8");
    }

    #[test]
    fn test_str_enc_testuser() {
        let result = str_enc("testuser", "1", "2", "3");
        assert_eq!(result, "D8D35E5019288C41EB6D16D13160C8DC");
    }

    #[test]
    fn test_str_enc_abcdefgh() {
        let result = str_enc("abcdefgh", "1", "2", "3");
        assert_eq!(result, "A9CF2704230383D1EEE0EB174D2B1826");
    }

    #[test]
    fn test_str_enc_hello() {
        let result = str_enc("hello", "1", "2", "3");
        assert_eq!(result, "8464FE022385FBA6D7ECB4832A17EF58");
    }

    #[test]
    fn test_str_enc_full() {
        // username + password + lt
        let data = "testusertestpassLT-TEST-1234-test";
        let result = str_enc(data, "1", "2", "3");
        assert_eq!(
            result,
            "D8D35E5019288C41EB6D16D13160C8DCD8D35E5019288C410303D47B562E51A923F565D94D86535E620B0E1BD16331DCC1BB5938DF9F2190F9D4022F34FC7594620FCF409D1B0A94"
        );
    }

    #[test]
    fn test_des_init() {
        let result = des_init("testuser", "testpass", "LT-TEST-1234-test");
        assert_eq!(
            result,
            "D8D35E5019288C41EB6D16D13160C8DCD8D35E5019288C410303D47B562E51A923F565D94D86535E620B0E1BD16331DCC1BB5938DF9F2190F9D4022F34FC7594620FCF409D1B0A94"
        );
    }

    #[test]
    fn test_block_consistency() {
        // str_enc("testuser") 应与 str_enc("testusertestpass...") 的前两个块一致
        let single = str_enc("testuser", "1", "2", "3");
        let full = str_enc("testusertestpassLT-TEST-1234-test", "1", "2", "3");
        assert!(full.starts_with(&single));
    }
}

