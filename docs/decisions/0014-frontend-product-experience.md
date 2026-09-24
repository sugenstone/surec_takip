# Frontend Product Experience Architecture

Tarih: 2026-09-23
Durum: STEP 19.5; yerel uygulama, final review için. Commit/push kapsam dışı.

## Başlangıç ve gözlenen sorunlar

Başlangıç `main`, HEAD ve origin/main: `d9a6ac55ded969640de55ebd0005507a59f0d8f3`.
Çalışma ağacı temizdi; dokuz migration çifti, 26 frontend unit testi ve altı
dosyada 24 E2E vardı. Bu HEAD'in hosted CI çalışması başarılıydı:
[35873700874](https://github.com/sugenstone/surec_takip/actions/runs/35873700874).
Starter ayrı repository olarak temizdi ve değiştirilmedi.

Kod değişmeden önce izole yerel test verisiyle desktop/mobile uygulama incelendi.
Somut bulgular:

- Root ve shell iç içe `main` ve yinelenen skip link üretiyordu.
- Workspace seçicisi sayfanın gerçek workspace bilgisini almıyordu; tema
  seçicisinin değeri kaydedilmiş temayı göstermiyordu.
- Mobil üst alan gereğinden uzundu; context, hesap ve gezinme ayrımı zayıftı.
- Project → Section → Work Item geçişinde ancestor yolu görünmüyordu.
- Sayfa başlıkları, status, form, hata ve card stilleri tekrar ediyordu.
- Section satırlarında çok sayıda eşit ağırlıklı işlem vardı. Sınırsız
  girinti derin ağaçlarda kullanılabilir alanı azaltıyordu; daraltma yoktu.
- Slug ve düzenleme alanları işin adı/hiyerarşisi kadar baskındı.
- Work Item görünümü kendi form/CSS dünyasına sahipti; ortak sayfa yapısı yoktu.

## Karar: iki gezinme düzeyi

Shell organizasyon/workspace seçimi, hesap kontrolleri ve workspace kapsamlı
Projects girişini taşır. Sayfa içindeki breadcrumb organizasyon → workspace →
project → recursive section → work item yolunu gösterir. İç içe sidebar yoktur.
Proje listesinden detayına, bölümden alt bölüme veya işçiliğe, oradan detaya
gidilir. Geçerli breadcrumb metni bağlantı değildir. Section ancestor
bağlantıları aynı project kapsamındaki API verisinden türetilir; bozuk/eksik
zincirde yol uydurulmaz. Mevcut section erişimi ayrıca doğrulanır.

### STEP 19.5B düzeltmesi: drill-down gezinme

Önceki sunum recursive section ağacını tek sayfada girintili satırlar ve
aç/kapat disclosure kontrolleriyle gösteriyordu; bu ürün amacı değildi.
Düzeltilmiş model:

"Recursive hierarchy is represented through URL-based drill-down
navigation. A page renders one hierarchy level and its direct children; the
UI does not recursively expand the entire descendant tree."

Her section kendi kalıcı URL'sine sahiptir
(`.../projects/{project}/sections/{section}`); proje sayfası yalnız kök
section'ları, section sayfası yalnız doğrudan alt bölümleri ve o section'ın
işçiliklerini gösterir. Derinlik breadcrumb ile anlaşılır; girinti veya
disclosure ile değil. Doğrudan URL, yenileme, ileri/geri geçmiş ve yeni sekme
aynı bağlamı üretir; hiyerarşi için istemci depolaması kullanılmaz.
Eski `.../sections/{id}/work-items` liste URL'si section sayfasına 307 ile
yönlenir; work item detay URL'si değişmez.

### STEP 19.5C: uygulama kabuğu ve ürün UI sistemi

Önceki shell tek bir üst şeritti; sayfalar listelerin altına gömülü kalıcı
oluşturma formları taşıyordu ve satır içi küçük buton dizileri vardı. STEP
19.5C kalıcı bir uygulama kabuğu kurar: masaüstünde persistent sidebar
(brand, org/workspace context seçicileri, primary nav, hesap bloğu + çıkış),
üstte topbar (mobil menü tetikleyicisi, gezinme durumu, locale ve tema
seçicileri). Mobil kırılımda aynı sidebar backdrop'lu off-canvas drawer olur;
Escape kapatır, gövde scroll kilitlenir, odak tetikleyiciye geri döner.

Oluşturma yüzeyleri artık liste içinde yaşamayan, birincil eylemle açılan
FormDrawer sheet'leridir (masaüstünde sağ taraflı native `<dialog>`,
mobilde görüş alanını neredeyse dolduran sheet). Escape/backdrop kapatır,
odak yönetimi native dialog tarafından sağlanır, backend hatasında taslağı
korur, başarıda kapanır. İkincil entity eylemleri satır içi butonlar yerine
paylaşılan ActionMenu overflow'unu kullanır (menu/menuitem rolleri, ok/Home/
End/Escape, dışarı tıklama, odak iadesi, danger yalnız yıkıcı eylemde).
Arşivleme ayrı ConfirmDialog akışında kalır.

Drill-down invariantı değişmez: proje sayfası yalnız kök section kartları,
section sayfası yalnız doğrudan alt bölümler ve kendi işçilikleri. Section
kartı gezinme nesnesidir (stretched link + chevron); yönetim eylemleri
section'ın kendi sayfasındaki ActionMenu'dedir. Sahte metrik yoktur: kartlar
yalnız gerçek API alanlarını (isim, slug, status, doğrudan-alt-öğe sayısı)
gösterir; STEP 20 process/timer/assignee verisi için kompozisyon yer tutar
ama placeholder çizmez. Sidebar'da yalnız gerçek hedefler (Projeler, Çalışma
Alanları) vardır; gelecek modüller sahte disabled öğe olarak gösterilmez.

## Ortak sayfa ve bileşen sınırı

`packages/ui`: PageHeader, StatusBadge, EmptyState, ConfirmDialog, FormDrawer,
ActionMenu ve semantic tokenlar. Bileşenler çevrilmiş metin alır; route,
tenant veya API bilmez.
`apps/web/src/app.css`: ortak sayfa, form, collection, action ve dialog stilleri.
Breadcrumbs uygulamanın route bilgisine ihtiyaç duyduğu için `$lib/ui` içindedir;
SectionBreadcrumbs aynı dizinde org→workspace→project→section zincirini kurar.
SectionCard/SectionCreateForm ve WorkItemCard/Form kendi feature dizinlerinde
kalır.

PageHeader context, başlık, açıklama, metadata ve action snippet alanlarını
sunmaktadır. Sayfa gövdesi gerçek içerik ve loading/empty/error durumlarını
taşır. Navigasyonda shell mevcut içeriği korur ve busy/status bildirir.
Tek main ve tek skip link vardır; içerik genişliği ve form genişliği tokenlıdır.

## Section ve Work Item sunumu

Doğrudan-alt-öğe seçimi, kardeş sıralaması, descendant dışlama ve ancestor yolu
saf fonksiyonlardır; görünümden bağımsız kaynak veri korunur ve bozuk veri
güvenli şekilde sönümlenir. Section card birincil olarak gezinme nesnesidir:
kartın tamamı section sayfasına bağlanır (stretched link), yönetim işlemleri
(yeniden adlandırma, taşıma, sıralama, arşiv/yeniden etkinleştirme) section'ın
kendi sayfasındadır; kart içinde iç içe etkileşimli öğe yoktur. Card görünümü
gerçek doğrudan-alt-öğe sayısını ikincil metin olarak gösterebilir.

Section sayfası aynı anda hem alt bölüm hem işçilik taşıyabilir; önce alt
bölümler, sonra işçilikler listelenir. Section oluşturma formu mevcut hiyerarşi
bağlamını kullanır (proje sayfasında kök, section sayfasında alt bölüm); parent
yine backend'de doğrulanır. Arşivlenmiş section sayfası salt-okunur kalır ve
yeniden etkinleştirme erişilebilir durur; işçilik API'si arşivli parent altında
404 döndürdüğü için işçilik listesi orada gösterilmez.

WorkItemCard gerçek isim, slug ve status gösterir. İsteğe bağlı children snippet
ileride yetkili API'den gelecek process özetini taşıyabilir; bugün boş alan,
sahte step, timer veya yüzde üretmez. SectionBreadcrumbs section ve işçilik
detayında aynı yolu kullanır. Detay formu başarısız kayıt durumunda draft'ı
korur.
İşçilik alanları ve komutları hydration tamamlanana kadar disabled kalır;
SSR görüntüsü üzerinde handler bağlanmadan normal HTML submit veya kaybolan
erken kullanıcı girdisi oluşması engellenir.
Shell seçicileri/komutları ve proje düzenleme/yaşam döngüsü komutları aynı hazır
olma kuralını izler. SSR içerik okunabilir kalır; bu kontrollerin hydration
öncesi kapalı olduğu JavaScript kapalı browser testiyle de doğrulanır.

## Eylem, status, form ve erişilebilirlik

Create/save primary; edit/cancel secondary; archive danger biçimindedir.
Archive native modal dialog içinde açık onay ister. İlk odak cancel'dadır;
Escape iptal eder ve odak tetikleyiciye döner; pending sırasında tekrar gönderim
ve kapatma engellenir. Backend hatası dialog içinde kalır.
StatusBadge metin ve renkten bağımsız işaret taşır. Active/completed/archived
renkleri semantic tokenlardan gelir. Form label, required, pending, server error
ve draft davranışı korunur; hata metinleri stable API code mapping ile çevrilir.

Context seçicileri sidebar'ın üstünde, hesap bloğu ve çıkış en alttadır;
mobilde aynı blok off-canvas drawer içinde korunur ve tetikleyici topbar'dadır.
Buton, input, summary kontrolleri en az 44px yüksekliğindedir. Uzun içerik
sarılır; focus-visible ve reduced-motion temel kuralları korunur. TR/EN anahtarları
eşdeğerdir. Dark/system tema mevcut token override mekanizmasını kullanır.

## Güvenlik ve etkilenmeyen sınırlar

URL + SSR parent context otoritedir. Context cookie'leri yalnız UX ipucudur.
Session HttpOnly cookie olarak backend'e SSR üzerinden iletilir; browser storage
kullanılmaz. Permission görünürlüğü bir kolaylıktır, backend enforcement'ın yerini
almaz. Logout mevcut backend session revoke çağrısını kullanır. Raw hata veya
cross-tenant kaynak bilgisi yeni UI üzerinden gösterilmez.

Backend, API DTO/contract, migration, RBAC, tenancy, audit ve realtime davranışı
değişmez. Yeni cache/global domain store yoktur. Work Item context loader'ı mevcut
project-scoped section endpoint'inden bir ek read yapar; ancestor listesi için
yeni endpoint veya raw-ID lookup açılmaz.

## Gelecek sınırı ve doğrulama

Process, timer, assignment, realtime, Active Processes sayfası ve TV yoktur.
İleride workspace düzeyindeki Active Processes girişi shell'e eklenebilir;
yetkili domain verisi, state/error kuralları ve backend olmadan görünür placeholder
oluşturulmaz. Work Item arşivini geri alma ve optimistic concurrency mevcut
backend sınırları nedeniyle bu frontend adımında eklenmez.

Unit testleri doğrudan-alt-öğe seçimini, kardeş sıralamasını, keyfi derinliği,
ancestor zincirini ve bozuk zinciri kapsar. E2E kapsamı 360/375/768/1280px ×
light/dark, gerçek drill-down geçişi (proje → kat → daire → işçilik), karışık
içerikli section, deep link/reload, tarayıcı ileri/geri, dil değişimi, draft
korunması ve archive cancel/Escape/focus/confirm davranışını içerir. Eski auth/tenant/member/lifecycle
senaryoları korunur. Son çalıştırma sonuçları final görev raporunda verilir;
bu ADR hosted STEP 19.5 CI başarısı iddia etmez.
