//! Oracle parity tests for image processing simulation in pure Elisp.
//!
//! Covers: 2D matrix as image, convolution with kernels (blur, sharpen,
//! edge-detect), histogram computation, threshold/binarize, erosion/dilation
//! (morphological), image rotate 90/180/270, crop and pad,
//! brightness/contrast adjust.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// 2D matrix as image + convolution with kernels
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_imgproc_convolution_kernels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Image is a vector of vectors (row-major). Pixel values 0-255.
  (fset 'neovm--img-make
    (lambda (rows cols val)
      (let ((img (make-vector rows nil))
            (r 0))
        (while (< r rows)
          (aset img r (make-vector cols val))
          (setq r (1+ r)))
        img)))

  (fset 'neovm--img-get
    (lambda (img r c)
      (aref (aref img r) c)))

  (fset 'neovm--img-set
    (lambda (img r c val)
      (aset (aref img r) c val)))

  (fset 'neovm--img-rows (lambda (img) (length img)))
  (fset 'neovm--img-cols (lambda (img) (length (aref img 0))))

  ;; 2D convolution with zero-padding. Kernel is a flat list + ksize.
  (fset 'neovm--img-convolve
    (lambda (img kernel ksize)
      (let* ((rows (funcall 'neovm--img-rows img))
             (cols (funcall 'neovm--img-cols img))
             (half (/ ksize 2))
             (out (funcall 'neovm--img-make rows cols 0))
             (kvec (vconcat kernel)))
        (let ((r 0))
          (while (< r rows)
            (let ((c 0))
              (while (< c cols)
                (let ((sum 0) (ki 0))
                  (let ((kr 0))
                    (while (< kr ksize)
                      (let ((kc 0))
                        (while (< kc ksize)
                          (let ((ir (+ r (- kr half)))
                                (ic (+ c (- kc half))))
                            (when (and (>= ir 0) (< ir rows)
                                       (>= ic 0) (< ic cols))
                              (setq sum (+ sum (* (aref kvec ki)
                                                  (funcall 'neovm--img-get img ir ic))))))
                          (setq ki (1+ ki))
                          (setq kc (1+ kc))))
                      (setq kr (1+ kr)))))
                (funcall 'neovm--img-set out r c sum)
                (setq c (1+ c))))
            (setq r (1+ r))))
        out)))

  (unwind-protect
      (let* (;; 5x5 test image with a bright center
             (img (funcall 'neovm--img-make 5 5 10))
             (_ (progn
                  (funcall 'neovm--img-set img 1 1 50)
                  (funcall 'neovm--img-set img 1 2 50)
                  (funcall 'neovm--img-set img 1 3 50)
                  (funcall 'neovm--img-set img 2 1 50)
                  (funcall 'neovm--img-set img 2 2 100)
                  (funcall 'neovm--img-set img 2 3 50)
                  (funcall 'neovm--img-set img 3 1 50)
                  (funcall 'neovm--img-set img 3 2 50)
                  (funcall 'neovm--img-set img 3 3 50)))
             ;; Box blur kernel (3x3, not normalized — sum of values)
             (blur-k '(1 1 1 1 1 1 1 1 1))
             (blurred (funcall 'neovm--img-convolve img blur-k 3))
             ;; Sharpen kernel
             (sharp-k '(0 -1 0 -1 5 -1 0 -1 0))
             (sharpened (funcall 'neovm--img-convolve img sharp-k 3))
             ;; Edge detect (Laplacian)
             (edge-k '(0 1 0 1 -4 1 0 1 0))
             (edges (funcall 'neovm--img-convolve img edge-k 3)))
        (list
         :original-center (funcall 'neovm--img-get img 2 2)
         :blurred-center (funcall 'neovm--img-get blurred 2 2)
         :blurred-corner (funcall 'neovm--img-get blurred 0 0)
         :sharpened-center (funcall 'neovm--img-get sharpened 2 2)
         :edge-center (funcall 'neovm--img-get edges 2 2)
         :edge-outside (funcall 'neovm--img-get edges 0 0)
         ;; Blur should spread: center value reduced relative to sum
         :blur-spreads (< (funcall 'neovm--img-get blurred 2 2)
                          (* 9 (funcall 'neovm--img-get img 2 2)))))
    (fmakunbound 'neovm--img-make)
    (fmakunbound 'neovm--img-get)
    (fmakunbound 'neovm--img-set)
    (fmakunbound 'neovm--img-rows)
    (fmakunbound 'neovm--img-cols)
    (fmakunbound 'neovm--img-convolve)))"#;
    let expect = expect_test::expect![[r#""ERR (void-variable sum)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Histogram computation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_imgproc_histogram() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--img-histogram
    (lambda (img num-bins max-val)
      "Compute histogram of pixel values in IMG.
Bins values into NUM-BINS buckets over [0, MAX-VAL]."
      (let* ((rows (length img))
             (hist (make-vector num-bins 0))
             (bin-width (/ (float max-val) num-bins)))
        (let ((r 0))
          (while (< r rows)
            (let ((row (aref img r))
                  (c 0)
                  (cols (length (aref img r))))
              (while (< c cols)
                (let* ((val (aref row c))
                       (bin (min (1- num-bins)
                                 (floor (/ (float val) bin-width)))))
                  (aset hist bin (1+ (aref hist bin))))
                (setq c (1+ c))))
            (setq r (1+ r))))
        (append hist nil))))

  (unwind-protect
      (let* (;; 4x4 image with known distribution
             (img (vector (vector 0 10 20 30)
                          (vector 40 50 60 70)
                          (vector 80 90 100 110)
                          (vector 120 130 200 255)))
             (hist-4 (funcall 'neovm--img-histogram img 4 256))
             (hist-8 (funcall 'neovm--img-histogram img 8 256))
             ;; Uniform image
             (uniform (vector (vector 128 128) (vector 128 128)))
             (hist-uni (funcall 'neovm--img-histogram uniform 4 256))
             ;; Total count should equal pixel count
             (total-4 (apply '+ hist-4))
             (total-8 (apply '+ hist-8)))
        (list
         :hist-4-bins hist-4
         :hist-8-bins hist-8
         :total-4 total-4
         :total-8 total-8
         :total-correct (and (= total-4 16) (= total-8 16))
         :uniform-hist hist-uni
         ;; All uniform pixels in same bin
         :uniform-single-bin (= (apply 'max hist-uni) 4)))
    (fmakunbound 'neovm--img-histogram)))"#;
    let expect = expect_test::expect![[
        r#""OK (:hist-4-bins (7 6 1 2) :hist-8-bins (4 3 3 3 1 0 1 1) :total-4 16 :total-8 16 :total-correct t :uniform-hist (0 0 4 0) :uniform-single-bin t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Threshold / binarize
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_imgproc_threshold() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--img-threshold
    (lambda (img thresh high low)
      "Binarize IMG: pixels >= THRESH become HIGH, else LOW."
      (let* ((rows (length img))
             (cols (length (aref img 0)))
             (out (make-vector rows nil))
             (r 0))
        (while (< r rows)
          (let ((row (aref img r))
                (new-row (make-vector cols 0))
                (c 0))
            (while (< c cols)
              (aset new-row c (if (>= (aref row c) thresh) high low))
              (setq c (1+ c)))
            (aset out r new-row))
          (setq r (1+ r)))
        out)))

  (unwind-protect
      (let* ((img (vector (vector 10 50 90 130)
                          (vector 170 210 30 70)
                          (vector 110 150 190 230)
                          (vector 255 0 128 64)))
             (bin-128 (funcall 'neovm--img-threshold img 128 255 0))
             (bin-64 (funcall 'neovm--img-threshold img 64 1 0))
             ;; Count white pixels in bin-128
             (white-count
              (let ((cnt 0) (r 0))
                (while (< r (length bin-128))
                  (let ((row (aref bin-128 r)) (c 0))
                    (while (< c (length row))
                      (when (= (aref row c) 255) (setq cnt (1+ cnt)))
                      (setq c (1+ c))))
                  (setq r (1+ r)))
                cnt)))
        (list
         :bin-128-row0 (append (aref bin-128 0) nil)
         :bin-128-row1 (append (aref bin-128 1) nil)
         :bin-128-row2 (append (aref bin-128 2) nil)
         :bin-128-row3 (append (aref bin-128 3) nil)
         :white-count-128 white-count
         :bin-64-row0 (append (aref bin-64 0) nil)
         ;; All-white: threshold at 0
         :all-white (let ((all (funcall 'neovm--img-threshold img 0 1 0))
                          (cnt 0) (r 0))
                      (while (< r (length all))
                        (let ((row (aref all r)) (c 0))
                          (while (< c (length row))
                            (setq cnt (+ cnt (aref row c)))
                            (setq c (1+ c))))
                        (setq r (1+ r)))
                      cnt)))
    (fmakunbound 'neovm--img-threshold)))"#;
    let expect = expect_test::expect![[
        r#""OK (:bin-128-row0 (0 0 0 255) :bin-128-row1 (255 255 0 0) :bin-128-row2 (0 255 255 255) :bin-128-row3 (255 0 255 0) :white-count-128 8 :bin-64-row0 (0 0 1 1) :all-white 16)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Erosion and dilation (morphological operations)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_imgproc_morphological() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Binary image morphological ops. 1 = foreground, 0 = background.
  ;; 3x3 cross structuring element.
  (fset 'neovm--img-erode
    (lambda (img)
      "Erode binary image with 3x3 cross SE.
Pixel is 1 only if center AND all 4-neighbors are 1."
      (let* ((rows (length img))
             (cols (length (aref img 0)))
             (out (make-vector rows nil))
             (r 0))
        (while (< r rows)
          (let ((new-row (make-vector cols 0)) (c 0))
            (while (< c cols)
              (if (and (= (aref (aref img r) c) 1)
                       (or (= r 0) (= (aref (aref img (1- r)) c) 1))
                       (or (= r (1- rows)) (= (aref (aref img (1+ r)) c) 1))
                       (or (= c 0) (= (aref (aref img r) (1- c)) 1))
                       (or (= c (1- cols)) (= (aref (aref img r) (1+ c)) 1)))
                  (aset new-row c 1)
                (aset new-row c 0))
              (setq c (1+ c)))
            (aset out r new-row))
          (setq r (1+ r)))
        out)))

  (fset 'neovm--img-dilate
    (lambda (img)
      "Dilate binary image with 3x3 cross SE.
Pixel is 1 if center OR any 4-neighbor is 1."
      (let* ((rows (length img))
             (cols (length (aref img 0)))
             (out (make-vector rows nil))
             (r 0))
        (while (< r rows)
          (let ((new-row (make-vector cols 0)) (c 0))
            (while (< c cols)
              (if (or (= (aref (aref img r) c) 1)
                      (and (> r 0) (= (aref (aref img (1- r)) c) 1))
                      (and (< r (1- rows)) (= (aref (aref img (1+ r)) c) 1))
                      (and (> c 0) (= (aref (aref img r) (1- c)) 1))
                      (and (< c (1- cols)) (= (aref (aref img r) (1+ c)) 1)))
                  (aset new-row c 1)
                (aset new-row c 0))
              (setq c (1+ c)))
            (aset out r new-row))
          (setq r (1+ r)))
        out)))

  (fset 'neovm--img-count-fg
    (lambda (img)
      (let ((cnt 0) (r 0))
        (while (< r (length img))
          (let ((row (aref img r)) (c 0))
            (while (< c (length row))
              (when (= (aref row c) 1) (setq cnt (1+ cnt)))
              (setq c (1+ c))))
          (setq r (1+ r)))
        cnt)))

  (unwind-protect
      (let* (;; 7x7 image with a cross pattern
             (img (vector (vector 0 0 0 1 0 0 0)
                          (vector 0 0 0 1 0 0 0)
                          (vector 0 0 0 1 0 0 0)
                          (vector 1 1 1 1 1 1 1)
                          (vector 0 0 0 1 0 0 0)
                          (vector 0 0 0 1 0 0 0)
                          (vector 0 0 0 1 0 0 0)))
             (eroded (funcall 'neovm--img-erode img))
             (dilated (funcall 'neovm--img-dilate img))
             (fg-orig (funcall 'neovm--img-count-fg img))
             (fg-eroded (funcall 'neovm--img-count-fg eroded))
             (fg-dilated (funcall 'neovm--img-count-fg dilated))
             ;; Opening = erode then dilate
             (opened (funcall 'neovm--img-dilate eroded))
             (fg-opened (funcall 'neovm--img-count-fg opened)))
        (list
         :fg-original fg-orig
         :fg-eroded fg-eroded
         :fg-dilated fg-dilated
         :fg-opened fg-opened
         ;; Erosion shrinks, dilation grows
         :erosion-shrinks (<= fg-eroded fg-orig)
         :dilation-grows (>= fg-dilated fg-orig)
         ;; Opening count
         :opened-leq-original (<= fg-opened fg-orig)
         ;; Eroded center row
         :eroded-row3 (append (aref eroded 3) nil)
         :dilated-row0 (append (aref dilated 0) nil)))
    (fmakunbound 'neovm--img-erode)
    (fmakunbound 'neovm--img-dilate)
    (fmakunbound 'neovm--img-count-fg)))"#;
    let expect = expect_test::expect![[
        r#""OK (:fg-original 13 :fg-eroded 1 :fg-dilated 33 :fg-opened 5 :erosion-shrinks t :dilation-grows t :opened-leq-original t :eroded-row3 (0 0 0 1 0 0 0) :dilated-row0 (0 0 1 1 1 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Image rotation: 90, 180, 270 degrees
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_imgproc_rotation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--img-rotate90
    (lambda (img)
      "Rotate image 90 degrees clockwise.
(r,c) -> (c, rows-1-r)"
      (let* ((rows (length img))
             (cols (length (aref img 0)))
             ;; New image: cols x rows
             (out (make-vector cols nil))
             (r 0))
        (while (< r cols)
          (aset out r (make-vector rows 0))
          (setq r (1+ r)))
        (setq r 0)
        (while (< r rows)
          (let ((c 0))
            (while (< c cols)
              (aset (aref out c) (- rows 1 r)
                    (aref (aref img r) c))
              (setq c (1+ c))))
          (setq r (1+ r)))
        out)))

  (fset 'neovm--img-rotate180
    (lambda (img) (funcall 'neovm--img-rotate90
                           (funcall 'neovm--img-rotate90 img))))

  (fset 'neovm--img-rotate270
    (lambda (img) (funcall 'neovm--img-rotate90
                           (funcall 'neovm--img-rotate180 img))))

  (fset 'neovm--img-to-list
    (lambda (img)
      (let ((result nil) (r 0))
        (while (< r (length img))
          (push (append (aref img r) nil) result)
          (setq r (1+ r)))
        (nreverse result))))

  (unwind-protect
      (let* ((img (vector (vector 1 2 3)
                          (vector 4 5 6)))
             (r90 (funcall 'neovm--img-rotate90 img))
             (r180 (funcall 'neovm--img-rotate180 img))
             (r270 (funcall 'neovm--img-rotate270 img))
             (r360 (funcall 'neovm--img-rotate90
                            (funcall 'neovm--img-rotate270 img))))
        (list
         :original (funcall 'neovm--img-to-list img)
         :rotated-90 (funcall 'neovm--img-to-list r90)
         :rotated-180 (funcall 'neovm--img-to-list r180)
         :rotated-270 (funcall 'neovm--img-to-list r270)
         ;; Dimensions after 90: 2x3 -> 3x2
         :r90-dims (list (length r90) (length (aref r90 0)))
         ;; 360 = identity
         :r360-identity (equal (funcall 'neovm--img-to-list r360)
                               (funcall 'neovm--img-to-list img))
         ;; 180 of 180 = identity
         :r180-twice (equal (funcall 'neovm--img-to-list
                                     (funcall 'neovm--img-rotate180 r180))
                            (funcall 'neovm--img-to-list img))))
    (fmakunbound 'neovm--img-rotate90)
    (fmakunbound 'neovm--img-rotate180)
    (fmakunbound 'neovm--img-rotate270)
    (fmakunbound 'neovm--img-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (:original ((1 2 3) (4 5 6)) :rotated-90 ((4 1) (5 2) (6 3)) :rotated-180 ((6 5 4) (3 2 1)) :rotated-270 ((3 6) (2 5) (1 4)) :r90-dims (3 2) :r360-identity t :r180-twice t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Crop and pad
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_imgproc_crop_and_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--img-crop
    (lambda (img r1 c1 r2 c2)
      "Crop image to region [r1,r2) x [c1,c2)."
      (let* ((out-rows (- r2 r1))
             (out-cols (- c2 c1))
             (out (make-vector out-rows nil))
             (r 0))
        (while (< r out-rows)
          (let ((new-row (make-vector out-cols 0))
                (c 0))
            (while (< c out-cols)
              (aset new-row c (aref (aref img (+ r1 r)) (+ c1 c)))
              (setq c (1+ c)))
            (aset out r new-row))
          (setq r (1+ r)))
        out)))

  (fset 'neovm--img-pad
    (lambda (img top bottom left right pad-val)
      "Pad image with PAD-VAL on all sides."
      (let* ((orig-rows (length img))
             (orig-cols (length (aref img 0)))
             (new-rows (+ orig-rows top bottom))
             (new-cols (+ orig-cols left right))
             (out (make-vector new-rows nil))
             (r 0))
        (while (< r new-rows)
          (aset out r (make-vector new-cols pad-val))
          (setq r (1+ r)))
        (setq r 0)
        (while (< r orig-rows)
          (let ((c 0))
            (while (< c orig-cols)
              (aset (aref out (+ r top)) (+ c left)
                    (aref (aref img r) c))
              (setq c (1+ c))))
          (setq r (1+ r)))
        out)))

  (fset 'neovm--img-to-list
    (lambda (img)
      (let ((result nil) (r 0))
        (while (< r (length img))
          (push (append (aref img r) nil) result)
          (setq r (1+ r)))
        (nreverse result))))

  (unwind-protect
      (let* ((img (vector (vector 1 2 3 4)
                          (vector 5 6 7 8)
                          (vector 9 10 11 12)))
             ;; Crop center 2x2
             (cropped (funcall 'neovm--img-crop img 0 1 2 3))
             ;; Pad with zeros (1 pixel all around)
             (padded (funcall 'neovm--img-pad img 1 1 1 1 0))
             ;; Crop the padded image to get original back
             (unpadded (funcall 'neovm--img-crop padded 1 1 4 5)))
        (list
         :original (funcall 'neovm--img-to-list img)
         :cropped (funcall 'neovm--img-to-list cropped)
         :padded (funcall 'neovm--img-to-list padded)
         :padded-dims (list (length padded) (length (aref padded 0)))
         ;; Unpad should recover original
         :unpad-recovers (equal (funcall 'neovm--img-to-list unpadded)
                                (funcall 'neovm--img-to-list img))
         ;; Crop dimensions correct
         :crop-dims (list (length cropped) (length (aref cropped 0)))))
    (fmakunbound 'neovm--img-crop)
    (fmakunbound 'neovm--img-pad)
    (fmakunbound 'neovm--img-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (:original ((1 2 3 4) (5 6 7 8) (9 10 11 12)) :cropped ((2 3) (6 7)) :padded ((0 0 0 0 0 0) (0 1 2 3 4 0) (0 5 6 7 8 0) (0 9 10 11 12 0) (0 0 0 0 0 0)) :padded-dims (5 6) :unpad-recovers t :crop-dims (2 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Brightness and contrast adjustment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_imgproc_brightness_contrast() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--img-clamp
    (lambda (val lo hi)
      (max lo (min hi val))))

  (fset 'neovm--img-adjust-brightness
    (lambda (img delta)
      "Add DELTA to every pixel, clamped to [0,255]."
      (let* ((rows (length img))
             (out (make-vector rows nil))
             (r 0))
        (while (< r rows)
          (let* ((row (aref img r))
                 (cols (length row))
                 (new-row (make-vector cols 0))
                 (c 0))
            (while (< c cols)
              (aset new-row c (funcall 'neovm--img-clamp (+ (aref row c) delta) 0 255))
              (setq c (1+ c)))
            (aset out r new-row))
          (setq r (1+ r)))
        out)))

  (fset 'neovm--img-adjust-contrast
    (lambda (img factor)
      "Scale pixel values around 128 by FACTOR (integer percentage / 100).
new = clamp(128 + (old - 128) * factor / 100)."
      (let* ((rows (length img))
             (out (make-vector rows nil))
             (r 0))
        (while (< r rows)
          (let* ((row (aref img r))
                 (cols (length row))
                 (new-row (make-vector cols 0))
                 (c 0))
            (while (< c cols)
              (let ((val (+ 128 (/ (* (- (aref row c) 128) factor) 100))))
                (aset new-row c (funcall 'neovm--img-clamp val 0 255)))
              (setq c (1+ c)))
            (aset out r new-row))
          (setq r (1+ r)))
        out)))

  (fset 'neovm--img-to-list
    (lambda (img)
      (let ((result nil) (r 0))
        (while (< r (length img))
          (push (append (aref img r) nil) result)
          (setq r (1+ r)))
        (nreverse result))))

  (unwind-protect
      (let* ((img (vector (vector 0 50 100 150 200 255)
                          (vector 30 80 128 180 220 240)))
             (bright+50 (funcall 'neovm--img-adjust-brightness img 50))
             (bright-50 (funcall 'neovm--img-adjust-brightness img -50))
             (high-contrast (funcall 'neovm--img-adjust-contrast img 200))
             (low-contrast (funcall 'neovm--img-adjust-contrast img 50))
             (zero-bright (funcall 'neovm--img-adjust-brightness img 0)))
        (list
         :original (funcall 'neovm--img-to-list img)
         :bright-plus-50 (funcall 'neovm--img-to-list bright+50)
         :bright-minus-50 (funcall 'neovm--img-to-list bright-50)
         :high-contrast (funcall 'neovm--img-to-list high-contrast)
         :low-contrast (funcall 'neovm--img-to-list low-contrast)
         ;; Zero brightness offset = identity
         :zero-bright-identity (equal (funcall 'neovm--img-to-list zero-bright)
                                      (funcall 'neovm--img-to-list img))
         ;; All values in [0, 255]
         :bright-clamped
         (let ((ok t) (r 0))
           (while (< r (length bright+50))
             (let ((row (aref bright+50 r)) (c 0))
               (while (< c (length row))
                 (when (or (< (aref row c) 0) (> (aref row c) 255))
                   (setq ok nil))
                 (setq c (1+ c))))
             (setq r (1+ r)))
           ok)))
    (fmakunbound 'neovm--img-clamp)
    (fmakunbound 'neovm--img-adjust-brightness)
    (fmakunbound 'neovm--img-adjust-contrast)
    (fmakunbound 'neovm--img-to-list)))"#;
    let expect = expect_test::expect![[
        r#""OK (:original ((0 50 100 150 200 255) (30 80 128 180 220 240)) :bright-plus-50 ((50 100 150 200 250 255) (80 130 178 230 255 255)) :bright-minus-50 ((0 0 50 100 150 205) (0 30 78 130 170 190)) :high-contrast ((0 0 72 172 255 255) (0 32 128 232 255 255)) :low-contrast ((64 89 114 139 164 191) (79 104 128 154 174 184)) :zero-bright-identity t :bright-clamped t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
