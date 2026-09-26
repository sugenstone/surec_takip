# Time Sessions (çalışma oturumları — emek süresi)

Tarih: 2026-09-29
Durum: Kabul edildi (STEP 21D)

## Bağlam

STEP 21A (ADR 0016) yürütme duvar saati (`started_at → completed_at/
cancelled_at`), STEP 21C (ADR 0018) sorumluluk ve aktör alanlarını çözdü.
Eksik kalan ayrı bir sorudur:

> "Bu yürütmede **kim, ne kadar süre fiilen çalıştı?**"

İnsan emeği duvar saatiyle aynı şey değildir: bir yürütme gece boyunca
bekleyebilir, birden çok işçi sırayla veya eşzamanlı çalışabilir, duraklama
duvar saatini durdurmaz. Bu yüzden emek ayrı bir kayıt modelidir:

`process_execution_time_sessions` — her satır, TEK bir işçinin TEK bir
denemede kesintisiz çalıştığı bir aralıktır.

## Kimlik sınırları

| Kavram         | Soru                           | Alan                                           |
| -------------- | ------------------------------ | ---------------------------------------------- |
| Worker (işçi)  | Emek kimin hesabına yazıldı?   | `worker_user_id`                               |
| Aktör          | Oturumu kim başlattı/kapattı?  | `started_by_user_id` / `ended_by_user_id`      |
| Assignee       | Süreçten kim sorumlu?          | `processes` / `process_executions` snapshot    |
| Yürütme sahibi | Denemeyi kim başlattı/bitirdi? | `started_by` / `completed_by` / `cancelled_by` |

Dördü de farklı kullanıcılar olabilir: Harun sorumludur, Abdullah denemeyi
başlatmıştır, Cihan çalışmaktadır, yönetici iptal edince oturum `ended_by =
yönetici` ile kapanır. V1 API'si self-service'tir — worker her zaman
oturum açmış kullanıcıdır (`worker_user_id` istemciden gelmez;
`deny_unknown_fields` taklidi 400 ile reddeder). Alan yine de ayrıdır ve
terminal geçişlerde `ended_by ≠ worker` fiilen ayrışır.

## Oturum yaşam döngüsü

- **Aç**: `ended_at IS NULL` satır INSERT; durum kolonu yok.
- **Duraklat**: satırı kapat — ayrı "paused" durumu yok.
- **Devam**: yeni satır INSERT — aralık asla geriye dönük uzatılmaz.
- **Otomatik kapanma**: yürütmenin COMPLETE/CANCEL işlemiyle aynı
  transaction'da o denemenin tüm açık oturumları `ended_at = geçiş zamanı`,
  `ended_by = geçiş aktörü` ile kapanır. Terminal denemede açık oturum
  kalamaz; bu yüzden `started_at` çifti DB saatiyle aynı transaction
  `now()`'sini paylaşır.

## Tek açık oturum kuralı (V1)

Bir worker aynı tenant içinde en fazla BİR açık oturum tutabilir — farklı
proje, iş kalemi, süreç veya denemede bile. Kural hem uygulama
ön-kontrolüyle (`ACTIVE_SESSION_EXISTS` 409) hem de partial unique index
ile (`(tenant_id, worker_user_id) WHERE ended_at IS NULL`) dayatılır:
yarış durumu 23505 olarak değil, aynı anlamsal hataya çevrilir.

Aynı denemede birden çok worker'ın açık oturumu olabilir — deneme başına
tek worker zorlaması bilinçli olarak YOKTUR (ekip işi gerçektir).

## Yetki ve durdurma sınırı

- `time_sessions:start`, `time_sessions:stop` — Owner + built-in Member'a
  organization scope'ta verilir; workspace üyeliği yine geçerlidir.
- Okuma (deneme geçmişi + iş kalemi açık-oturum listesi) sıradan workspace
  erişimidir; ayrı okuma izni yok.
- V1 self-service durdurma: oturumu yalnızca **worker'ın kendisi**
  kapatabilir — `time_sessions:stop` sahibi başkasının oturumunu
  kapatamaz (403). Terminal geçişler bu kurala tabi değildir: yönetici
  yürütmeyi tamamlayınca başkasının açık oturumu da `ended_by = yönetici`
  ile kapanır — bu bir hak değil, geçişin atomik yan etkisidir.

## Scope ve tarihsel bütünlük

Session satırı tam (tenant, workspace, project, section, work_item,
process) zinciri taşır ve composite FK ile denemenin gerçekten o zincire
ait olduğunu DB seviyesinde ispatlar (011'in `processes` üzerindeki
desenin aynısı; 014 `process_executions` için `process_executions_scope_id_key`
ekler).

`worker_user_id` doğrudan `users(id)`'ye bağlanır — üyelik satırına DEĞİL.
STEP 21C dersiyle aynı: üyeliği kaldırılan işçinin geçmiş emeği kaybolmaz,
yalnızca eligibility işe yaramaz hale gelir. `ended_by`/`started_by` da
`users(id)`'dir.

## İstemci sözleşmesi

- Tüm zamanlar DB saatinden gelir; her yanıt `server_time` taşır.
- İstemci tarafındaki sayaç display-only'dir; veritabanına saniyede yazma
  yoktur (mimari kural).
- Hata sıralaması execution'larla aynıdır: 401 → 404 → 403 → 400 → 409.
- Aynı oturumun tekrar kapatılması `409 STATE_CONFLICT`'tir — kapatma
  yazısı `ended_at IS NULL` şartıyla tek seferliktir.

## Denenen ve reddedilen alternatifler

- Execution üzerinde pause alanları/sayaçları: çok-worker geçmişini,
  worker-başına aralıkları ve raporlanabilir satırları temsil edemez;
  "kim çalıştı" sorusunu cevaplayamaz.
- Tek worker/deneme kısıtı: gerçek ekip çalışmasını engeller; V1 kuralı
  işçi başına tekliktir, deneme başına değil.
- `status='paused'` kolonu: duraklatılmış süre zaten satırın kapanmasıyla
  ifade edilir; ikinci bir durum makinesi gereksizdir.
- Genel "timer" tablosu süreç-dışı varlıklara: gereksiz soyutlama; V1
  yalnızca deneme altındadır.

## Erteleme (explicitly out of scope)

Başkasının adına oturum açma (yönetici girişi), manuel düzeltme/silme,
stop reason, audit tablosu, worker-başına kişisel rapor, realtime event,
offline kuyruk, mola tipleri. Hepsi ayrı fazlardır; şema ve API bunları
engellemez ama varsaymaz da.

## Sınır koşulları

- Session aralığı execution'ın ömrünü aşamaz: terminal geçiş hepsini
  kapatır; terminal denemede start `409 STATE_CONFLICT`'tir.
- Üyeliği kaldırılan işçinin açık oturumu satırda kalır ve ancak terminal
  geçişle veya kendi stop'uyla kapanır (üyelik artık context'ten
  geçemezse otomatik kapanma garantisi terminal geçiştedir).
- Process reassignment açık oturuma dokunmaz: sorumluluk ile emek ayrı
  domain'lerdir.
- `deny_unknown_fields` sayesinde worker/timestamp spoofing 400'dür;
  `ACTIVE_SESSION_EXISTS` asla sessiz auto-stop üretmez.
