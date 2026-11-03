# Sổ style edit AYP

Đây là những style AYP đã chốt sau khi phân tích các video mẫu và duyệt nhiều clip thật. Tài liệu chỉ ghi
nguyên lý và tham số khởi điểm; khi dựng vẫn phải chỉnh lại bằng mắt. Nguyên tắc gốc:
**mọi thứ đặt lên màn hình đều phải có ý nghĩa**, phụ đề đẹp và dễ đọc như sản phẩm Apple.

FilmCraft là công cụ, còn biên tập viên (người hoặc Claude qua MCP) quyết phần nghĩa và sáng tạo: chọn
đoạn, chia cụm phụ đề, chọn minh hoạ. Phần máy móc như thời gian, kiểm luật, render là lệnh trong
FilmCraft.

## 1. Chọn style theo loại clip, TRƯỚC khi dựng

Mỗi clip chỉ dùng **một ngôn ngữ**: một kiểu chuyển cảnh, một cặp font, một bảng màu. Hiệu ứng nặng
thì dùng thật tiết kiệm.

| Loại clip | Style | Chuyển cảnh / nhịp |
|---|---|---|
| Học viên kể chuyện cảm xúc | Trầm, ít bố cục, chữ phân cấp (từ khoá to đậm, từ nối nhỏ mảnh), nhạc piano | Blur 4–6 frame rồi hard cut ở chỗ chuyển ý; 4–8 s mỗi ý; lặng ~0,4 s trước câu nặng |
| Mẹo / giải thích nhanh | Nhanh, nhiều chữ, nhấn hiệu ứng ở 7–8 điểm | Cắt 1–2 s; zoom blur / strobe đảo màu / glitch / flash dùng tiết kiệm; sound design trầm (hit, riser) |
| Phỏng vấn / chuyên gia | Ấm, sạch, thẻ infographic nền kem làm "chỗ thở" | Chỉ một kiểu chuyển cảnh (flash trắng 3 frame, cắt ở đỉnh); 4–5 s/cảnh |
| Quote / trích dẫn | Chữ chuyển động đơn sắc | Mask nở/thu đặt vào chỗ ngắt hơi |
| Quảng cáo khoá / CTA | Chữ khổng lồ, dồn nhịp cuối | Đổi hình ~0,45 s ở 3 s cuối, giữ khung cuối 0,5 s; tắt âm 0,45 s rồi "boom" |
| Clip mood / nhạc | Tối, ánh sáng đối nghịch, build → drop | Không nháy sáng nhanh (≥ 3 Hz là nguy cơ cho người nhạy sáng) |

Số đo trên video mẫu: điểm cắt bám đầu cụm nói (±10–170 ms), không bám nhạc. Trong clip nhịp nhanh,
khoảng 2/3 số điểm chuyển là cắt thẳng.

**Tham số tái hiện** (điểm khởi đầu):

| Hiệu ứng | Tham số |
|---|---|
| Blur-out | Gaussian 0→25–35 px trong 4 frame cuối + scale 100→104 % |
| Blur-through | 0→25–40→0 trong ~0,27 s + scale 112 % |
| Zoom-blur punch | 0→100→0 trong 3 frame + Exposure +1,5 |
| Strobe | Invert bật ở frame 0, 1, 5 |
| Glitch | Glitch + RGB Split 8–12 px, 10 frame |
| Chữ blur-in | blur 25→0 + opacity 40→100 trong 6–8 frame |
| Chữ trượt | X ±300 → 0, 7–8 frame, ease-out |

## 2. Phụ đề

Luật đầy đủ, mỗi luật có test, nằm trong [CLAUDE.md](../CLAUDE.md) mục *AYP caption rules*. Tóm tắt:

- **Mỗi cụm phụ đề xuất hiện là một ý trọn.** Không bao giờ ghép đuôi câu trước với đầu câu sau.
- 6–10 chữ mỗi cụm. Vừa một dòng (≤ 21 ký tự ở cỡ 48, khung dọc) thì không xuống dòng. Hai dòng phải
  cân, và không dòng nào chỉ có một chữ.
- Không tách từ ghép, số với đơn vị, hay cụm tiếng Anh ("layoff", "follow up").
- Ngắt dòng ở chỗ mở vế câu (nếu/thì/mà/vì/để/nhưng…). Tiểu từ cuối câu (à, ạ, nhé, nha, nè…) đi
  với câu trước nó.
- Sửa chữ nghe nhầm TRƯỚC khi ngắt dòng.
- Kiểu mặc định: Be Vietnam Pro 48, chữ trắng viền đen, từ khoá và số tô vàng `#ffd200`, bóng đổ nhẹ.
- Chỗ có minh hoạ (motion, typo) thì tạm ẩn phụ đề.
- Trước khi xuất, xuất khung kiểm ở các phụ đề dài: cụm ≥ 19 ký tự có thể bị bẻ thành 3 dòng khi render.

## 3. Cắt clip

- Cắt theo ý trọn câu, đừng cắt sát đến mức câu vụn. Chỉ bỏ chỗ lặp dài ≥ 0,6 s, không tạo đoạn < 1,5 s.
- Điểm cắt bám chỗ lặng (RMS thấp nhất gần mốc chữ). Mốc chữ khớp bằng wav2vec2
  (`tools/ayp-align`) chính xác hơn mốc whisper.
- Nghe lại từng điểm cắt để bắt chữ bị lẹm hoặc dính.
- Đề xuất danh sách clip cho người duyệt trước rồi mới cắt; mỗi clip một sequence riêng.
- Mở bằng chi tiết đắt nhất (con số, câu chuyện cụ thể). Giữ câu đặt bối cảnh để người xem hiểu
  đúng chuyện gì đang xảy ra.
- Clip kể chuyện phải có **diễn biến**: cảnh cụ thể có lời thoại, cách xử lý, kết quả. Ghép mỗi câu
  tổng kết thì clip sẽ "cục cục". Dài tối đa 5 phút.

## 4. Bố cục dọc 1080×1920 từ ghi hình Zoom

| Nguồn | Bố cục |
|---|---|
| Gallery: 2 cam cùng hiện, không slide | **Chia đều 960/960**: trainer trên, học viên dưới, cố định (không đổi theo người nói); mỗi nửa canh theo mặt; phụ đề ở đường chia |
| Speaker view: 1 người trên hình | **Khung 4:5 giữa** (1080×1350, sát hai lề, tâm y ≈ 760) + nền là chính khung đó phóng phủ kín, gaussian blur 60, sáng −20; phụ đề ở khoảng 2/3 khung, không che mặt |

- Canh khung theo mặt **từng đoạn**, vì mặt đổi chỗ giữa các đoạn. Dùng Apple Vision dò mặt ~2 fps.
- Zoom chuyển cam trễ ~1,5–2 s sau khi người mới nói, nên đầu đoạn có thể còn cam người trước.
- Whisper không tách người nói. Ở các câu hỏi hoặc câu chuyển ý, soát ai đang nói bằng độ mở môi.
- Giữa hai phần (học viên kể → nhận xét của trainer): nhịp nghỉ 0,6 s chìm về đen. Phụ đề phải hết
  trước khi hình tối.

## 5. Motion và typo

**Khi có minh hoạ:** video bên dưới mờ và tối đi rồi trả lại (Gaussian Blur 18, sáng −35, bão hoà 55;
vào 0,25 s, ra 0,3 s). Làm mờ cả video, không chỉ một track. Trong khoảng đó không có phụ đề.
Comp chỉ chứa đồ hoạ, không tự phủ nền đen.

**B – Keynote kiểu Apple** (style chủ đạo): chữ Be Vietnam Pro SemiBold ~132 trắng, căn giữa, vào bằng
blur 40→0 + scale 94→100 (~0,45 s); chữ trước lùi về 55 % opacity; nhãn nhỏ viết HOA giãn chữ phía
trên; không màu phụ.

**Kính mờ + typo glow** (clip kể chuyện): nền kính mờ có hạt texture (video dưới blur 32 + tối); chữ
Be Vietnam Pro Black glow vừa, vàng `#ffd200` cho số và từ khoá. Element biểu đồ glow cùng kiểu. Vào/ra
kiểu bay (overshoot nhẹ, motion blur), mỗi đoạn một kiểu vào/ra khác nhau. Mã dựng lại:
[`tools/ayp-motion/glass_lib.py`](../tools/ayp-motion/glass_lib.py) (`glass_panel`, `text`, `glow`,
`fly_in`, `fly_out`, `float_idle`) chạy qua cổng điều khiển EffectCraft 9880 (`ec.py`).

**Typo combo phát sáng** (cho từ khoá, số liệu, hook, CTA; không thay phụ đề):
- Một cụm = 1 từ khoá TO (sans đậm hoặc condensed) + 1 từ script thư pháp đè lệch ~30 % chiều cao +
  1–3 chữ phụ nhỏ bám mép. Tổng 3–7 chữ, khối chặt; tỉ lệ cỡ chữ ~ 1 : 0,45 : 0,25.
- Font: script (Great Vibes / Pinyon / Dancing Script), sans đen (Be Vietnam Pro Black, Montserrat
  ExtraBold), condensed (Anton/Oswald). Thử dấu chồng (ặ ổ ữ) trước khi dùng.
- Màu: mỗi cụm 1 hue + trắng; chữ chính gradient 2 tông cùng họ; glow ngoài cùng màu chữ (blur lớn,
  opacity 60–80 %). Màu theo nghĩa: tiền/giá vàng, công nghệ xanh, nóng/tăng đỏ-cam, sang/xanh lục.
- Nhịp (~1,5–2,5 s/cụm): từng chữ vào lệch 80–150 ms (blur 20→0 + opacity 0→1 + scale 115→100,
  ease-out); script hiện bằng mask quét trái→phải; số đếm hoặc bật overshoot; giữ 0,8–1,2 s; cả cụm
  ra cùng lúc (blur + fade 0,2 s). Trên footage sáng thì thêm nền tối mờ.
- Dựng bằng HTML/GSAP (HyperFrames) hoặc EffectCraft → ProRes 4444 alpha → track trên trong FilmCraft.

Tránh: neon, gradient tím, 3D bóng, sparkle "AI", emoji bay, nhiều kiểu chuyển cảnh lặt vặt.

## 6. Đóng gói clip

- Headline mở 0–3,5 s kiểu glassmorphism: thẻ kính bo góc 36, chỉ nhoè hình **bên trong** thẻ, phủ
  `#1a1a1a` ~45 %, viền trắng 2 px ~40 %, ánh sáng mép trên. Ngoài thẻ hình vẫn rõ (chỉ tối −15).
  Dòng chính vàng ExtraBold to nhất. Phụ đề trùng headline thì ẩn.
- Tên và nghề của người nói ở góc dưới trái, ~10 s sau headline. Dòng nguồn nhỏ in nghiêng ở góc trên
  trái suốt video.
- Chuyển cảnh giữa các ý: whip hoặc zoom blur + **một** tiếng whoosh.
- Nhạc nhỏ hơn giọng ~20 dB, đỉnh mix khoảng −1,8 dB. Nhạc thương mại có rủi ro Content-ID.
- Outro: quảng cáo khoá học (hình mờ đen → ~1,5 s đen → quảng cáo hiện dần).

## 7. Quy trình clip học viên

Chốt với người duyệt ở các bước 1, 2, 3, 3b, 5 rồi mới sang bước sau.

1. **Ghi chú cả buổi** từ transcript: ai hỏi gì, ai nói gì, theo mốc thời gian. Liệt kê mọi chỗ chưa
   chắc (nghe không rõ, không chắc ai nói) để chốt trước, đừng tự đoán.
2. **Chốt giá trị:** người xem phải HIỂU và bị HOOK. Chấm theo 6 tiêu chí: bối cảnh, sự cố cụ thể, lời
   thoại/suy nghĩ thật, cách xử lý, kết quả, bài học.
3. **Chốt mạch thành script:** hook → người đó là ai → vấn đề → vì sao đi học → áp dụng gì → kết quả
   → câu chốt. Câu hỏi của trainer làm cầu nối.
   - 3b. **Kế hoạch biên tập:** chọn style theo mục 1. Theo từng đoạn script, ghi chỗ highlight, footage
     chèn, chuyển cảnh và SFX.
4. **Cắt** theo mục 3.
5. **Soát nội dung:** câu trước → câu sau có logic chưa, ý trọn chưa, có chữ treo không ("điểm đó",
   "bạn đó").
6. **Bố cục + phụ đề** theo mục 4 và mục 2.
7. Người duyệt tự cắt tinh trên timeline thì **sửa tại chỗ**, không dựng lại.
8. **Đóng gói** theo mục 6.
9. **Xuất** vào `<thư mục làm việc>/Render/<tên buổi>/`, có in phụ đề. Duyệt góp ý qua
   *Sequence ▸ Duyệt góp ý (AYP)…*.

Câu chuyện quá riêng tư thì phải xin phép người kể trước khi đăng.

## 8. Âm thanh (SFX, nhạc)

Repo **không** chứa SFX hay nhạc: phần lớn là gói của bên thứ ba, không được phép phân phối lại. Ai
cần thì xin chủ repo gửi riêng. Hãy để vào `<thư mục làm việc>/Editor Material/` để các script dựng
tìm thấy.
