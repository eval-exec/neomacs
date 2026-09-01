//! UTF-8 / multibyte *eight-bit char-charset matrix* (bytes 128-255).
//!
//! Exhaustive per-byte probe: `char-charset` of every raw-byte character.
//! Confirmed root cause (Theme 1): Neomacs classifies every eight-bit char as
//! `unicode`, GNU classifies it as `eight-bit`. One focused #[test] per byte
//! pins exactly which byte values diverge.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

macro_rules! cs {
    ($name:ident, $byte:expr) => {
        #[test]
        fn $name() {
            return_if_neovm_enable_oracle_proptest_not_set!();
            crate::common::assert_oracle_parity(&format!(
                "(char-charset (unibyte-char-to-multibyte {}))",
                $byte
            ));
        }
    };
}

cs!(div_utf8_eightbit_charset_b128, 128);
cs!(div_utf8_eightbit_charset_b129, 129);
cs!(div_utf8_eightbit_charset_b130, 130);
cs!(div_utf8_eightbit_charset_b131, 131);
cs!(div_utf8_eightbit_charset_b132, 132);
cs!(div_utf8_eightbit_charset_b133, 133);
cs!(div_utf8_eightbit_charset_b134, 134);
cs!(div_utf8_eightbit_charset_b135, 135);
cs!(div_utf8_eightbit_charset_b136, 136);
cs!(div_utf8_eightbit_charset_b137, 137);
cs!(div_utf8_eightbit_charset_b138, 138);
cs!(div_utf8_eightbit_charset_b139, 139);
cs!(div_utf8_eightbit_charset_b140, 140);
cs!(div_utf8_eightbit_charset_b141, 141);
cs!(div_utf8_eightbit_charset_b142, 142);
cs!(div_utf8_eightbit_charset_b143, 143);
cs!(div_utf8_eightbit_charset_b144, 144);
cs!(div_utf8_eightbit_charset_b145, 145);
cs!(div_utf8_eightbit_charset_b146, 146);
cs!(div_utf8_eightbit_charset_b147, 147);
cs!(div_utf8_eightbit_charset_b148, 148);
cs!(div_utf8_eightbit_charset_b149, 149);
cs!(div_utf8_eightbit_charset_b150, 150);
cs!(div_utf8_eightbit_charset_b151, 151);
cs!(div_utf8_eightbit_charset_b152, 152);
cs!(div_utf8_eightbit_charset_b153, 153);
cs!(div_utf8_eightbit_charset_b154, 154);
cs!(div_utf8_eightbit_charset_b155, 155);
cs!(div_utf8_eightbit_charset_b156, 156);
cs!(div_utf8_eightbit_charset_b157, 157);
cs!(div_utf8_eightbit_charset_b158, 158);
cs!(div_utf8_eightbit_charset_b159, 159);
cs!(div_utf8_eightbit_charset_b160, 160);
cs!(div_utf8_eightbit_charset_b161, 161);
cs!(div_utf8_eightbit_charset_b162, 162);
cs!(div_utf8_eightbit_charset_b163, 163);
cs!(div_utf8_eightbit_charset_b164, 164);
cs!(div_utf8_eightbit_charset_b165, 165);
cs!(div_utf8_eightbit_charset_b166, 166);
cs!(div_utf8_eightbit_charset_b167, 167);
cs!(div_utf8_eightbit_charset_b168, 168);
cs!(div_utf8_eightbit_charset_b169, 169);
cs!(div_utf8_eightbit_charset_b170, 170);
cs!(div_utf8_eightbit_charset_b171, 171);
cs!(div_utf8_eightbit_charset_b172, 172);
cs!(div_utf8_eightbit_charset_b173, 173);
cs!(div_utf8_eightbit_charset_b174, 174);
cs!(div_utf8_eightbit_charset_b175, 175);
cs!(div_utf8_eightbit_charset_b176, 176);
cs!(div_utf8_eightbit_charset_b177, 177);
cs!(div_utf8_eightbit_charset_b178, 178);
cs!(div_utf8_eightbit_charset_b179, 179);
cs!(div_utf8_eightbit_charset_b180, 180);
cs!(div_utf8_eightbit_charset_b181, 181);
cs!(div_utf8_eightbit_charset_b182, 182);
cs!(div_utf8_eightbit_charset_b183, 183);
cs!(div_utf8_eightbit_charset_b184, 184);
cs!(div_utf8_eightbit_charset_b185, 185);
cs!(div_utf8_eightbit_charset_b186, 186);
cs!(div_utf8_eightbit_charset_b187, 187);
cs!(div_utf8_eightbit_charset_b188, 188);
cs!(div_utf8_eightbit_charset_b189, 189);
cs!(div_utf8_eightbit_charset_b190, 190);
cs!(div_utf8_eightbit_charset_b191, 191);
cs!(div_utf8_eightbit_charset_b192, 192);
cs!(div_utf8_eightbit_charset_b193, 193);
cs!(div_utf8_eightbit_charset_b194, 194);
cs!(div_utf8_eightbit_charset_b195, 195);
cs!(div_utf8_eightbit_charset_b196, 196);
cs!(div_utf8_eightbit_charset_b197, 197);
cs!(div_utf8_eightbit_charset_b198, 198);
cs!(div_utf8_eightbit_charset_b199, 199);
cs!(div_utf8_eightbit_charset_b200, 200);
cs!(div_utf8_eightbit_charset_b201, 201);
cs!(div_utf8_eightbit_charset_b202, 202);
cs!(div_utf8_eightbit_charset_b203, 203);
cs!(div_utf8_eightbit_charset_b204, 204);
cs!(div_utf8_eightbit_charset_b205, 205);
cs!(div_utf8_eightbit_charset_b206, 206);
cs!(div_utf8_eightbit_charset_b207, 207);
cs!(div_utf8_eightbit_charset_b208, 208);
cs!(div_utf8_eightbit_charset_b209, 209);
cs!(div_utf8_eightbit_charset_b210, 210);
cs!(div_utf8_eightbit_charset_b211, 211);
cs!(div_utf8_eightbit_charset_b212, 212);
cs!(div_utf8_eightbit_charset_b213, 213);
cs!(div_utf8_eightbit_charset_b214, 214);
cs!(div_utf8_eightbit_charset_b215, 215);
cs!(div_utf8_eightbit_charset_b216, 216);
cs!(div_utf8_eightbit_charset_b217, 217);
cs!(div_utf8_eightbit_charset_b218, 218);
cs!(div_utf8_eightbit_charset_b219, 219);
cs!(div_utf8_eightbit_charset_b220, 220);
cs!(div_utf8_eightbit_charset_b221, 221);
cs!(div_utf8_eightbit_charset_b222, 222);
cs!(div_utf8_eightbit_charset_b223, 223);
cs!(div_utf8_eightbit_charset_b224, 224);
cs!(div_utf8_eightbit_charset_b225, 225);
cs!(div_utf8_eightbit_charset_b226, 226);
cs!(div_utf8_eightbit_charset_b227, 227);
cs!(div_utf8_eightbit_charset_b228, 228);
cs!(div_utf8_eightbit_charset_b229, 229);
cs!(div_utf8_eightbit_charset_b230, 230);
cs!(div_utf8_eightbit_charset_b231, 231);
cs!(div_utf8_eightbit_charset_b232, 232);
cs!(div_utf8_eightbit_charset_b233, 233);
cs!(div_utf8_eightbit_charset_b234, 234);
cs!(div_utf8_eightbit_charset_b235, 235);
cs!(div_utf8_eightbit_charset_b236, 236);
cs!(div_utf8_eightbit_charset_b237, 237);
cs!(div_utf8_eightbit_charset_b238, 238);
cs!(div_utf8_eightbit_charset_b239, 239);
cs!(div_utf8_eightbit_charset_b240, 240);
cs!(div_utf8_eightbit_charset_b241, 241);
cs!(div_utf8_eightbit_charset_b242, 242);
cs!(div_utf8_eightbit_charset_b243, 243);
cs!(div_utf8_eightbit_charset_b244, 244);
cs!(div_utf8_eightbit_charset_b245, 245);
cs!(div_utf8_eightbit_charset_b246, 246);
cs!(div_utf8_eightbit_charset_b247, 247);
cs!(div_utf8_eightbit_charset_b248, 248);
cs!(div_utf8_eightbit_charset_b249, 249);
cs!(div_utf8_eightbit_charset_b250, 250);
cs!(div_utf8_eightbit_charset_b251, 251);
cs!(div_utf8_eightbit_charset_b252, 252);
cs!(div_utf8_eightbit_charset_b253, 253);
cs!(div_utf8_eightbit_charset_b254, 254);
cs!(div_utf8_eightbit_charset_b255, 255);
