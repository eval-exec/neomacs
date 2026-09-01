//! UTF-8 / multibyte *eight-bit string-bytes matrix* (bytes 128-255).
//!
//! Exhaustive per-byte probe: `string-bytes` of decoding each byte (invalid
//! UTF-8) via decode-coding-string. Confirmed root cause (Theme 2):
//! decode-coding-string recovery stores each eight-bit char in 3 bytes in
//! Neomacs vs 2 in GNU. One focused #[test] per byte.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

macro_rules! sb {
    ($name:ident, $byte:expr) => {
        #[test]
        fn $name() {
            return_if_neovm_enable_oracle_proptest_not_set!();
            crate::common::assert_oracle_parity(&format!(
                "(string-bytes (decode-coding-string (unibyte-string {}) 'utf-8))",
                $byte
            ));
        }
    };
}

sb!(div_utf8_eightbit_bytes_b128, 128);
sb!(div_utf8_eightbit_bytes_b129, 129);
sb!(div_utf8_eightbit_bytes_b130, 130);
sb!(div_utf8_eightbit_bytes_b131, 131);
sb!(div_utf8_eightbit_bytes_b132, 132);
sb!(div_utf8_eightbit_bytes_b133, 133);
sb!(div_utf8_eightbit_bytes_b134, 134);
sb!(div_utf8_eightbit_bytes_b135, 135);
sb!(div_utf8_eightbit_bytes_b136, 136);
sb!(div_utf8_eightbit_bytes_b137, 137);
sb!(div_utf8_eightbit_bytes_b138, 138);
sb!(div_utf8_eightbit_bytes_b139, 139);
sb!(div_utf8_eightbit_bytes_b140, 140);
sb!(div_utf8_eightbit_bytes_b141, 141);
sb!(div_utf8_eightbit_bytes_b142, 142);
sb!(div_utf8_eightbit_bytes_b143, 143);
sb!(div_utf8_eightbit_bytes_b144, 144);
sb!(div_utf8_eightbit_bytes_b145, 145);
sb!(div_utf8_eightbit_bytes_b146, 146);
sb!(div_utf8_eightbit_bytes_b147, 147);
sb!(div_utf8_eightbit_bytes_b148, 148);
sb!(div_utf8_eightbit_bytes_b149, 149);
sb!(div_utf8_eightbit_bytes_b150, 150);
sb!(div_utf8_eightbit_bytes_b151, 151);
sb!(div_utf8_eightbit_bytes_b152, 152);
sb!(div_utf8_eightbit_bytes_b153, 153);
sb!(div_utf8_eightbit_bytes_b154, 154);
sb!(div_utf8_eightbit_bytes_b155, 155);
sb!(div_utf8_eightbit_bytes_b156, 156);
sb!(div_utf8_eightbit_bytes_b157, 157);
sb!(div_utf8_eightbit_bytes_b158, 158);
sb!(div_utf8_eightbit_bytes_b159, 159);
sb!(div_utf8_eightbit_bytes_b160, 160);
sb!(div_utf8_eightbit_bytes_b161, 161);
sb!(div_utf8_eightbit_bytes_b162, 162);
sb!(div_utf8_eightbit_bytes_b163, 163);
sb!(div_utf8_eightbit_bytes_b164, 164);
sb!(div_utf8_eightbit_bytes_b165, 165);
sb!(div_utf8_eightbit_bytes_b166, 166);
sb!(div_utf8_eightbit_bytes_b167, 167);
sb!(div_utf8_eightbit_bytes_b168, 168);
sb!(div_utf8_eightbit_bytes_b169, 169);
sb!(div_utf8_eightbit_bytes_b170, 170);
sb!(div_utf8_eightbit_bytes_b171, 171);
sb!(div_utf8_eightbit_bytes_b172, 172);
sb!(div_utf8_eightbit_bytes_b173, 173);
sb!(div_utf8_eightbit_bytes_b174, 174);
sb!(div_utf8_eightbit_bytes_b175, 175);
sb!(div_utf8_eightbit_bytes_b176, 176);
sb!(div_utf8_eightbit_bytes_b177, 177);
sb!(div_utf8_eightbit_bytes_b178, 178);
sb!(div_utf8_eightbit_bytes_b179, 179);
sb!(div_utf8_eightbit_bytes_b180, 180);
sb!(div_utf8_eightbit_bytes_b181, 181);
sb!(div_utf8_eightbit_bytes_b182, 182);
sb!(div_utf8_eightbit_bytes_b183, 183);
sb!(div_utf8_eightbit_bytes_b184, 184);
sb!(div_utf8_eightbit_bytes_b185, 185);
sb!(div_utf8_eightbit_bytes_b186, 186);
sb!(div_utf8_eightbit_bytes_b187, 187);
sb!(div_utf8_eightbit_bytes_b188, 188);
sb!(div_utf8_eightbit_bytes_b189, 189);
sb!(div_utf8_eightbit_bytes_b190, 190);
sb!(div_utf8_eightbit_bytes_b191, 191);
sb!(div_utf8_eightbit_bytes_b192, 192);
sb!(div_utf8_eightbit_bytes_b193, 193);
sb!(div_utf8_eightbit_bytes_b194, 194);
sb!(div_utf8_eightbit_bytes_b195, 195);
sb!(div_utf8_eightbit_bytes_b196, 196);
sb!(div_utf8_eightbit_bytes_b197, 197);
sb!(div_utf8_eightbit_bytes_b198, 198);
sb!(div_utf8_eightbit_bytes_b199, 199);
sb!(div_utf8_eightbit_bytes_b200, 200);
sb!(div_utf8_eightbit_bytes_b201, 201);
sb!(div_utf8_eightbit_bytes_b202, 202);
sb!(div_utf8_eightbit_bytes_b203, 203);
sb!(div_utf8_eightbit_bytes_b204, 204);
sb!(div_utf8_eightbit_bytes_b205, 205);
sb!(div_utf8_eightbit_bytes_b206, 206);
sb!(div_utf8_eightbit_bytes_b207, 207);
sb!(div_utf8_eightbit_bytes_b208, 208);
sb!(div_utf8_eightbit_bytes_b209, 209);
sb!(div_utf8_eightbit_bytes_b210, 210);
sb!(div_utf8_eightbit_bytes_b211, 211);
sb!(div_utf8_eightbit_bytes_b212, 212);
sb!(div_utf8_eightbit_bytes_b213, 213);
sb!(div_utf8_eightbit_bytes_b214, 214);
sb!(div_utf8_eightbit_bytes_b215, 215);
sb!(div_utf8_eightbit_bytes_b216, 216);
sb!(div_utf8_eightbit_bytes_b217, 217);
sb!(div_utf8_eightbit_bytes_b218, 218);
sb!(div_utf8_eightbit_bytes_b219, 219);
sb!(div_utf8_eightbit_bytes_b220, 220);
sb!(div_utf8_eightbit_bytes_b221, 221);
sb!(div_utf8_eightbit_bytes_b222, 222);
sb!(div_utf8_eightbit_bytes_b223, 223);
sb!(div_utf8_eightbit_bytes_b224, 224);
sb!(div_utf8_eightbit_bytes_b225, 225);
sb!(div_utf8_eightbit_bytes_b226, 226);
sb!(div_utf8_eightbit_bytes_b227, 227);
sb!(div_utf8_eightbit_bytes_b228, 228);
sb!(div_utf8_eightbit_bytes_b229, 229);
sb!(div_utf8_eightbit_bytes_b230, 230);
sb!(div_utf8_eightbit_bytes_b231, 231);
sb!(div_utf8_eightbit_bytes_b232, 232);
sb!(div_utf8_eightbit_bytes_b233, 233);
sb!(div_utf8_eightbit_bytes_b234, 234);
sb!(div_utf8_eightbit_bytes_b235, 235);
sb!(div_utf8_eightbit_bytes_b236, 236);
sb!(div_utf8_eightbit_bytes_b237, 237);
sb!(div_utf8_eightbit_bytes_b238, 238);
sb!(div_utf8_eightbit_bytes_b239, 239);
sb!(div_utf8_eightbit_bytes_b240, 240);
sb!(div_utf8_eightbit_bytes_b241, 241);
sb!(div_utf8_eightbit_bytes_b242, 242);
sb!(div_utf8_eightbit_bytes_b243, 243);
sb!(div_utf8_eightbit_bytes_b244, 244);
sb!(div_utf8_eightbit_bytes_b245, 245);
sb!(div_utf8_eightbit_bytes_b246, 246);
sb!(div_utf8_eightbit_bytes_b247, 247);
sb!(div_utf8_eightbit_bytes_b248, 248);
sb!(div_utf8_eightbit_bytes_b249, 249);
sb!(div_utf8_eightbit_bytes_b250, 250);
sb!(div_utf8_eightbit_bytes_b251, 251);
sb!(div_utf8_eightbit_bytes_b252, 252);
sb!(div_utf8_eightbit_bytes_b253, 253);
sb!(div_utf8_eightbit_bytes_b254, 254);
sb!(div_utf8_eightbit_bytes_b255, 255);
