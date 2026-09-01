//! Advanced oracle parity tests for bitwise operation patterns:
//! `ash` with positive/negative counts, `logand`/`logior`/`logxor`/`lognot`
//! combinations, bit field extraction/insertion, bitmask flag manipulation,
//! IP address packing/unpacking, color channel extraction, and combined
//! bitwise algorithm patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// ash (arithmetic shift) advanced patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ash_power_of_two_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use ash to generate powers of two, verify against expt
    let form = r#"(let ((results nil))
  (dotimes (i 20)
    (let ((via-ash (ash 1 i))
          (via-expt (expt 2 i)))
      (setq results (cons (list i via-ash via-expt (= via-ash via-expt))
                          results))))
  (let ((all-match t))
    (dolist (r results)
      (unless (nth 3 r)
        (setq all-match nil)))
    (list all-match
          (ash 1 0)
          (ash 1 10)
          (ash 1 20)
          (ash 1 30))))"#;
    let expect = expect_test::expect![r#""OK (t 1 1024 1048576 1073741824)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_ash_right_shift_sign_extension() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Arithmetic right shift preserves sign (sign extension)
    let form = r#"(list
  ;; Positive values: right shift fills with 0
  (ash 255 -1)    ;; 127
  (ash 255 -4)    ;; 15
  (ash 1024 -10)  ;; 1
  ;; Negative values: right shift fills with 1 (sign extension)
  (ash -1 -1)     ;; -1 (all bits set, stays -1)
  (ash -256 -4)   ;; -16
  (ash -128 -7)   ;; -1
  (ash -1024 -5)  ;; -32
  ;; Shift right more bits than value has
  (ash 42 -100)   ;; 0
  (ash -42 -100)  ;; -1
  ;; Combined left then right (lossy round-trip)
  (let ((x 255))
    (ash (ash x 8) -8))
  ;; Negative left-then-right
  (let ((x -100))
    (ash (ash x 4) -4)))"#;
    let expect = expect_test::expect![r#""OK (127 15 1 -1 -16 -1 -32 0 -1 255 -100)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_ash_multiply_divide_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // ash left by n == multiply by 2^n (for positive values)
    // ash right by n == floor division by 2^n
    let form = r#"(let ((test-vals '(0 1 7 42 100 255 1000 65535))
      (results nil))
  (dolist (v test-vals)
    (let ((left3 (ash v 3))
          (mul8 (* v 8))
          (right3 (ash v -3))
          (div8 (/ v 8)))
      (setq results (cons (list v
                                (= left3 mul8)
                                (= right3 div8)
                                left3 right3)
                          results))))
  (nreverse results))"#;
    let expect = expect_test::expect![
        r#""OK ((0 t t 0 0) (1 t t 8 0) (7 t t 56 0) (42 t t 336 5) (100 t t 800 12) (255 t t 2040 31) (1000 t t 8000 125) (65535 t t 524280 8191))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bit field extraction and insertion (pack/unpack)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitfield_pack_unpack_rgb() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pack and unpack RGB color channels into a single integer
    // Format: 0x00RRGGBB
    let form = r#"(progn
  (fset 'neovm--test-pack-rgb
    (lambda (r g b)
      (logior (ash (logand r #xff) 16)
              (ash (logand g #xff) 8)
              (logand b #xff))))

  (fset 'neovm--test-unpack-rgb
    (lambda (packed)
      (list (logand (ash packed -16) #xff)
            (logand (ash packed -8) #xff)
            (logand packed #xff))))

  (fset 'neovm--test-blend-rgb
    (lambda (c1 c2 alpha)
      ;; alpha is 0-256, where 256 = fully c1, 0 = fully c2
      (let ((r1 (logand (ash c1 -16) #xff))
            (g1 (logand (ash c1 -8) #xff))
            (b1 (logand c1 #xff))
            (r2 (logand (ash c2 -16) #xff))
            (g2 (logand (ash c2 -8) #xff))
            (b2 (logand c2 #xff)))
        (let ((r (ash (+ (* r1 alpha) (* r2 (- 256 alpha))) -8))
              (g (ash (+ (* g1 alpha) (* g2 (- 256 alpha))) -8))
              (b (ash (+ (* b1 alpha) (* b2 (- 256 alpha))) -8)))
          (funcall 'neovm--test-pack-rgb r g b)))))

  (unwind-protect
      (let ((red   (funcall 'neovm--test-pack-rgb 255 0 0))
            (green (funcall 'neovm--test-pack-rgb 0 255 0))
            (blue  (funcall 'neovm--test-pack-rgb 0 0 255))
            (white (funcall 'neovm--test-pack-rgb 255 255 255))
            (coral (funcall 'neovm--test-pack-rgb 255 127 80)))
        (list
          ;; Basic pack/unpack roundtrip
          (funcall 'neovm--test-unpack-rgb red)
          (funcall 'neovm--test-unpack-rgb green)
          (funcall 'neovm--test-unpack-rgb blue)
          (funcall 'neovm--test-unpack-rgb white)
          (funcall 'neovm--test-unpack-rgb coral)
          ;; Pack produces expected hex values
          (= red #xff0000)
          (= green #x00ff00)
          (= blue #x0000ff)
          ;; Blend red and blue at 50%
          (funcall 'neovm--test-unpack-rgb
            (funcall 'neovm--test-blend-rgb red blue 128))
          ;; Blend white and black at 25%
          (funcall 'neovm--test-unpack-rgb
            (funcall 'neovm--test-blend-rgb white 0 64))))
    (fmakunbound 'neovm--test-pack-rgb)
    (fmakunbound 'neovm--test-unpack-rgb)
    (fmakunbound 'neovm--test-blend-rgb)))"#;
    let expect = expect_test::expect![
        r#""OK ((255 0 0) (0 255 0) (0 0 255) (255 255 255) (255 127 80) t t t (127 0 127) (63 63 63))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bitfield_ip_address_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pack/unpack IPv4 addresses as 32-bit integers, compute subnet masks
    let form = r#"(progn
  (fset 'neovm--test-ip-pack
    (lambda (a b c d)
      (logior (ash a 24) (ash b 16) (ash c 8) d)))

  (fset 'neovm--test-ip-unpack
    (lambda (ip)
      (list (logand (ash ip -24) #xff)
            (logand (ash ip -16) #xff)
            (logand (ash ip -8) #xff)
            (logand ip #xff))))

  (fset 'neovm--test-subnet-mask
    (lambda (prefix-len)
      ;; Create mask with prefix-len leading 1 bits
      ;; e.g., /24 -> 0xFFFFFF00
      (let ((mask 0))
        (dotimes (i prefix-len)
          (setq mask (logior mask (ash 1 (- 31 i)))))
        mask)))

  (fset 'neovm--test-network-addr
    (lambda (ip mask)
      (logand ip mask)))

  (fset 'neovm--test-broadcast-addr
    (lambda (ip mask)
      (logior ip (lognot mask))))

  (fset 'neovm--test-same-subnet
    (lambda (ip1 ip2 mask)
      (= (logand ip1 mask) (logand ip2 mask))))

  (unwind-protect
      (let* ((ip1 (funcall 'neovm--test-ip-pack 192 168 1 100))
             (ip2 (funcall 'neovm--test-ip-pack 192 168 1 200))
             (ip3 (funcall 'neovm--test-ip-pack 192 168 2 100))
             (mask24 (funcall 'neovm--test-subnet-mask 24))
             (mask16 (funcall 'neovm--test-subnet-mask 16)))
        (list
          ;; Unpack roundtrip
          (funcall 'neovm--test-ip-unpack ip1)
          (funcall 'neovm--test-ip-unpack ip2)
          ;; Subnet mask for /24
          (funcall 'neovm--test-ip-unpack mask24)
          ;; Network address
          (funcall 'neovm--test-ip-unpack
            (funcall 'neovm--test-network-addr ip1 mask24))
          ;; Same subnet checks
          (funcall 'neovm--test-same-subnet ip1 ip2 mask24)  ;; t
          (funcall 'neovm--test-same-subnet ip1 ip3 mask24)  ;; nil
          (funcall 'neovm--test-same-subnet ip1 ip3 mask16)  ;; t
          ;; Host count in /24 subnet
          (let ((host-bits (- 32 24)))
            (- (ash 1 host-bits) 2))))  ;; 254 hosts
    (fmakunbound 'neovm--test-ip-pack)
    (fmakunbound 'neovm--test-ip-unpack)
    (fmakunbound 'neovm--test-subnet-mask)
    (fmakunbound 'neovm--test-network-addr)
    (fmakunbound 'neovm--test-broadcast-addr)
    (fmakunbound 'neovm--test-same-subnet)))"#;
    let expect = expect_test::expect![
        r#""OK ((192 168 1 100) (192 168 1 200) (255 255 255 0) (192 168 1 0) t nil t 254)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bitmask flag register with named operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitmask_permission_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Unix-like permission system using bitmasks
    let form = r#"(progn
  ;; Permission bits
  (defvar neovm--test-perm-read    #o444)
  (defvar neovm--test-perm-write   #o222)
  (defvar neovm--test-perm-exec    #o111)
  (defvar neovm--test-perm-owner-r #o400)
  (defvar neovm--test-perm-owner-w #o200)
  (defvar neovm--test-perm-owner-x #o100)
  (defvar neovm--test-perm-group-r #o040)
  (defvar neovm--test-perm-group-w #o020)
  (defvar neovm--test-perm-group-x #o010)
  (defvar neovm--test-perm-other-r #o004)
  (defvar neovm--test-perm-other-w #o002)
  (defvar neovm--test-perm-other-x #o001)

  (fset 'neovm--test-has-perm
    (lambda (mode perm)
      (not (= 0 (logand mode perm)))))

  (fset 'neovm--test-add-perm
    (lambda (mode perm)
      (logior mode perm)))

  (fset 'neovm--test-remove-perm
    (lambda (mode perm)
      (logand mode (lognot perm))))

  (fset 'neovm--test-toggle-perm
    (lambda (mode perm)
      (logxor mode perm)))

  (fset 'neovm--test-perm-string
    (lambda (mode)
      (let ((chars nil))
        ;; Owner
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-owner-r) ?r ?-) chars))
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-owner-w) ?w ?-) chars))
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-owner-x) ?x ?-) chars))
        ;; Group
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-group-r) ?r ?-) chars))
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-group-w) ?w ?-) chars))
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-group-x) ?x ?-) chars))
        ;; Other
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-other-r) ?r ?-) chars))
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-other-w) ?w ?-) chars))
        (setq chars (cons (if (funcall 'neovm--test-has-perm mode neovm--test-perm-other-x) ?x ?-) chars))
        (concat (nreverse chars)))))

  (unwind-protect
      (let ((mode-755 (logior neovm--test-perm-owner-r neovm--test-perm-owner-w neovm--test-perm-owner-x
                               neovm--test-perm-group-r neovm--test-perm-group-x
                               neovm--test-perm-other-r neovm--test-perm-other-x))
            (mode-644 (logior neovm--test-perm-owner-r neovm--test-perm-owner-w
                               neovm--test-perm-group-r
                               neovm--test-perm-other-r)))
        (list
          ;; Permission strings
          (funcall 'neovm--test-perm-string mode-755)
          (funcall 'neovm--test-perm-string mode-644)
          ;; Check specific permissions
          (funcall 'neovm--test-has-perm mode-755 neovm--test-perm-owner-x)  ;; t
          (funcall 'neovm--test-has-perm mode-644 neovm--test-perm-owner-x)  ;; nil
          ;; Add execute for all to 644
          (funcall 'neovm--test-perm-string
            (funcall 'neovm--test-add-perm mode-644 neovm--test-perm-exec))
          ;; Remove write from owner on 755
          (funcall 'neovm--test-perm-string
            (funcall 'neovm--test-remove-perm mode-755 neovm--test-perm-owner-w))
          ;; Toggle group-write on 755
          (funcall 'neovm--test-perm-string
            (funcall 'neovm--test-toggle-perm mode-755 neovm--test-perm-group-w))
          ;; Numeric values
          mode-755
          mode-644))
    (fmakunbound 'neovm--test-has-perm)
    (fmakunbound 'neovm--test-add-perm)
    (fmakunbound 'neovm--test-remove-perm)
    (fmakunbound 'neovm--test-toggle-perm)
    (fmakunbound 'neovm--test-perm-string)
    (makunbound 'neovm--test-perm-read)
    (makunbound 'neovm--test-perm-write)
    (makunbound 'neovm--test-perm-exec)
    (makunbound 'neovm--test-perm-owner-r)
    (makunbound 'neovm--test-perm-owner-w)
    (makunbound 'neovm--test-perm-owner-x)
    (makunbound 'neovm--test-perm-group-r)
    (makunbound 'neovm--test-perm-group-w)
    (makunbound 'neovm--test-perm-group-x)
    (makunbound 'neovm--test-perm-other-r)
    (makunbound 'neovm--test-perm-other-w)
    (makunbound 'neovm--test-perm-other-x)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"rwxr-xr-x\" \"rw-r--r--\" t nil \"rwxr-xr-x\" \"r-xr-xr-x\" \"rwxrwxr-x\" 493 420)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Bitwise algorithms: CRC-like checksum, bit reversal, gray code
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitwise_algorithms_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Bit reversal of an 8-bit value
  (fset 'neovm--test-reverse-bits8
    (lambda (n)
      (let ((result 0))
        (dotimes (i 8)
          (when (not (= 0 (logand n (ash 1 i))))
            (setq result (logior result (ash 1 (- 7 i))))))
        result)))

  ;; Binary to Gray code conversion: gray = n XOR (n >> 1)
  (fset 'neovm--test-to-gray
    (lambda (n)
      (logxor n (ash n -1))))

  ;; Gray code to binary conversion
  (fset 'neovm--test-from-gray
    (lambda (gray)
      (let ((n gray)
            (mask (ash gray -1)))
        (while (> mask 0)
          (setq n (logxor n mask))
          (setq mask (ash mask -1)))
        n)))

  ;; Simple checksum: XOR all bytes of a packed 32-bit value
  (fset 'neovm--test-xor-checksum
    (lambda (value)
      (logxor (logand (ash value -24) #xff)
              (logand (ash value -16) #xff)
              (logand (ash value -8) #xff)
              (logand value #xff))))

  ;; Count leading zeros (8-bit)
  (fset 'neovm--test-clz8
    (lambda (n)
      (let ((count 0) (val (logand n #xff)))
        (if (= val 0)
            8
          (progn
            (while (= 0 (logand val (ash 1 (- 7 count))))
              (setq count (1+ count)))
            count)))))

  ;; Find highest set bit position (0-indexed, -1 if zero)
  (fset 'neovm--test-highest-bit
    (lambda (n)
      (if (= n 0) -1
        (let ((pos 0) (val n))
          (while (> (ash val -1) 0)
            (setq val (ash val -1))
            (setq pos (1+ pos)))
          pos))))

  (unwind-protect
      (list
        ;; Bit reversal
        (funcall 'neovm--test-reverse-bits8 #b10110001)
        (funcall 'neovm--test-reverse-bits8 #b11111111)
        (funcall 'neovm--test-reverse-bits8 #b00000000)
        (funcall 'neovm--test-reverse-bits8 #b10000000)
        ;; Double reversal is identity
        (= (funcall 'neovm--test-reverse-bits8
             (funcall 'neovm--test-reverse-bits8 #b10110100))
           #b10110100)
        ;; Gray code roundtrip
        (let ((roundtrip-ok t))
          (dotimes (i 32)
            (unless (= i (funcall 'neovm--test-from-gray
                           (funcall 'neovm--test-to-gray i)))
              (setq roundtrip-ok nil)))
          roundtrip-ok)
        ;; Gray code: adjacent values differ by exactly 1 bit
        (let ((one-bit-diff t))
          (dotimes (i 15)
            (let ((diff (logxor (funcall 'neovm--test-to-gray i)
                                (funcall 'neovm--test-to-gray (1+ i)))))
              ;; diff should be a power of 2 (exactly one bit set)
              (unless (and (> diff 0)
                           (= 0 (logand diff (1- diff))))
                (setq one-bit-diff nil))))
          one-bit-diff)
        ;; XOR checksum
        (funcall 'neovm--test-xor-checksum #x12345678)
        (funcall 'neovm--test-xor-checksum #xffffffff)
        (funcall 'neovm--test-xor-checksum #x00000000)
        ;; CLZ
        (funcall 'neovm--test-clz8 #b10000000)   ;; 0
        (funcall 'neovm--test-clz8 #b00010000)   ;; 3
        (funcall 'neovm--test-clz8 #b00000001)   ;; 7
        (funcall 'neovm--test-clz8 0)             ;; 8
        ;; Highest bit
        (funcall 'neovm--test-highest-bit 1)      ;; 0
        (funcall 'neovm--test-highest-bit 255)    ;; 7
        (funcall 'neovm--test-highest-bit 1024)   ;; 10
        (funcall 'neovm--test-highest-bit 0))     ;; -1
    (fmakunbound 'neovm--test-reverse-bits8)
    (fmakunbound 'neovm--test-to-gray)
    (fmakunbound 'neovm--test-from-gray)
    (fmakunbound 'neovm--test-xor-checksum)
    (fmakunbound 'neovm--test-clz8)
    (fmakunbound 'neovm--test-highest-bit)))"#;
    let expect = expect_test::expect![r#""OK (141 255 0 1 t t t 8 0 0 0 3 7 8 0 7 10 -1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// De Morgan's laws and boolean algebra via bitwise ops
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitwise_boolean_algebra() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((a #xABCD)
      (b #x1234)
      (c #xFF00))
  (list
    ;; De Morgan: ~(A & B) == ~A | ~B
    (= (lognot (logand a b))
       (logior (lognot a) (lognot b)))
    ;; De Morgan: ~(A | B) == ~A & ~B
    (= (lognot (logior a b))
       (logand (lognot a) (lognot b)))
    ;; Distributive: A & (B | C) == (A & B) | (A & C)
    (= (logand a (logior b c))
       (logior (logand a b) (logand a c)))
    ;; Distributive: A | (B & C) == (A | B) & (A | C)
    (= (logior a (logand b c))
       (logand (logior a b) (logior a c)))
    ;; XOR is its own inverse: (A ^ B) ^ B == A
    (= (logxor (logxor a b) b) a)
    ;; Complement of complement: ~~A == A
    (= (lognot (lognot a)) a)
    ;; Absorption: A | (A & B) == A
    (= (logior a (logand a b)) a)
    ;; Absorption: A & (A | B) == A
    (= (logand a (logior a b)) a)
    ;; XOR commutativity and associativity
    (= (logxor a (logxor b c))
       (logxor (logxor a b) c))
    ;; A & ~A == 0
    (= (logand a (lognot a)) -1)  ;; Actually -1 for fixnums due to infinite precision
    ))"#;
    // Note: the last one checks that logand with lognot doesn't give 0
    // for arbitrary-precision integers. Let's fix to proper test.
    let form_fixed = r#"(let ((a #xABCD)
      (b #x1234)
      (c #xFF00))
  (list
    ;; De Morgan: ~(A & B) == ~A | ~B
    (= (lognot (logand a b))
       (logior (lognot a) (lognot b)))
    ;; De Morgan: ~(A | B) == ~A & ~B
    (= (lognot (logior a b))
       (logand (lognot a) (lognot b)))
    ;; Distributive: A & (B | C) == (A & B) | (A & C)
    (= (logand a (logior b c))
       (logior (logand a b) (logand a c)))
    ;; Distributive: A | (B & C) == (A | B) & (A | C)
    (= (logior a (logand b c))
       (logand (logior a b) (logior a c)))
    ;; XOR is its own inverse: (A ^ B) ^ B == A
    (= (logxor (logxor a b) b) a)
    ;; Complement of complement: ~~A == A
    (= (lognot (lognot a)) a)
    ;; Absorption: A | (A & B) == A
    (= (logior a (logand a b)) a)
    ;; Absorption: A & (A | B) == A
    (= (logand a (logior a b)) a)
    ;; XOR commutativity and associativity
    (= (logxor a (logxor b c))
       (logxor (logxor a b) c))
    ;; Idempotent: A | A == A, A & A == A
    (= (logior a a) a)
    (= (logand a a) a)))"#;
    let expect = expect_test::expect![r#""OK (t t t t t t t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form_fixed, expect);
}

// ---------------------------------------------------------------------------
// Bitfield-based compact data structure: date packing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitfield_date_packing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Pack year (12 bits), month (4 bits), day (5 bits), hour (5 bits),
    // minute (6 bits) into a single integer
    let form = r#"(progn
  (fset 'neovm--test-pack-datetime
    (lambda (year month day hour minute)
      (logior (ash (logand year #xfff) 20)
              (ash (logand month #xf) 16)
              (ash (logand day #x1f) 11)
              (ash (logand hour #x1f) 6)
              (logand minute #x3f))))

  (fset 'neovm--test-unpack-datetime
    (lambda (packed)
      (list (logand (ash packed -20) #xfff)   ;; year
            (logand (ash packed -16) #xf)      ;; month
            (logand (ash packed -11) #x1f)     ;; day
            (logand (ash packed -6) #x1f)      ;; hour
            (logand packed #x3f))))            ;; minute

  (fset 'neovm--test-datetime-cmp
    (lambda (dt1 dt2)
      ;; Packed datetimes compare correctly with simple integer comparison
      (cond ((< dt1 dt2) -1)
            ((> dt1 dt2) 1)
            (t 0))))

  (unwind-protect
      (let ((dt1 (funcall 'neovm--test-pack-datetime 2026 3 2 14 30))
            (dt2 (funcall 'neovm--test-pack-datetime 2026 3 2 15 0))
            (dt3 (funcall 'neovm--test-pack-datetime 2025 12 31 23 59))
            (dt4 (funcall 'neovm--test-pack-datetime 2026 1 1 0 0)))
        (list
          ;; Roundtrip
          (funcall 'neovm--test-unpack-datetime dt1)
          (funcall 'neovm--test-unpack-datetime dt2)
          (funcall 'neovm--test-unpack-datetime dt3)
          (funcall 'neovm--test-unpack-datetime dt4)
          ;; Comparison: later time > earlier time
          (funcall 'neovm--test-datetime-cmp dt1 dt2)   ;; -1 (earlier)
          (funcall 'neovm--test-datetime-cmp dt2 dt1)   ;; 1 (later)
          (funcall 'neovm--test-datetime-cmp dt1 dt1)   ;; 0 (equal)
          ;; Year rollover: 2025-12-31 < 2026-01-01
          (funcall 'neovm--test-datetime-cmp dt3 dt4)   ;; -1
          ;; Edge values
          (funcall 'neovm--test-unpack-datetime
            (funcall 'neovm--test-pack-datetime 0 1 1 0 0))
          (funcall 'neovm--test-unpack-datetime
            (funcall 'neovm--test-pack-datetime 4095 12 31 23 59))))
    (fmakunbound 'neovm--test-pack-datetime)
    (fmakunbound 'neovm--test-unpack-datetime)
    (fmakunbound 'neovm--test-datetime-cmp)))"#;
    let expect = expect_test::expect![
        r#""OK ((2026 3 2 14 30) (2026 3 2 15 0) (2025 12 31 23 59) (2026 1 1 0 0) -1 1 0 -1 (0 1 1 0 0) (4095 12 31 23 59))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
