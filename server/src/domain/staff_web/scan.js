(function () {
    const businessId = __BUSINESS_ID__;
    const video      = document.getElementById('video');
    const statusEl   = document.getElementById('status');
    const modeLabel  = document.getElementById('mode-label');
    const card       = document.getElementById('card');
    let scanning     = true;
    let mode         = 'stamp'; // 'stamp' | 'activate'

    // Wire mode buttons here so setMode stays in closure scope and
    // the HTML has no onclick attributes (which CSP blocks without unsafe-hashes).
    document.getElementById('btn-stamp').addEventListener('click',    () => setMode('stamp'));
    document.getElementById('btn-activate').addEventListener('click', () => setMode('activate'));

    function setMode(m) {
        mode = m;
        scanning = true;
        document.getElementById('btn-stamp').classList.toggle('active',    m === 'stamp');
        document.getElementById('btn-activate').classList.toggle('active', m === 'activate');
        modeLabel.textContent = m === 'stamp' ? 'stamp mode' : 'activate cup mode';
        statusEl.textContent  = m === 'stamp'
            ? 'point camera at customer QR'
            : 'scan the QR on the cup sticker';
    }

    // Feature detection: BarcodeDetector is available on Chrome, Android, and Edge.
    // iOS Safari does not support it — the file-input fallback handles those devices.
    //
    // Manual test steps for the iOS fallback:
    //   1. Open /staff/scan on an iPhone in Safari (or iOS Chrome)
    //   2. Confirm the camera viewfinder is replaced by a "scan QR code" button
    //   3. Tap the button — iOS should open the camera in capture mode
    //   4. Point at a customer QR stamp token → confirm stamp is recorded
    //   5. Switch to "activate cup" mode → scan an NFC companion QR → confirm activation
    if ('BarcodeDetector' in window) {
        navigator.mediaDevices.getUserMedia({ video: { facingMode: 'environment' } })
            .then(stream => { video.srcObject = stream; startNativeScanner(); })
            .catch(() => { statusEl.textContent = 'camera access denied'; });
    } else {
        startFallbackScanner();
    }

    // ── Native scanner (BarcodeDetector — Chrome, Android, Edge) ─────────────
    function startNativeScanner() {
        const detector = new BarcodeDetector({ formats: ['qr_code'] });
        async function tick() {
            if (!scanning) return;
            try {
                const codes = await detector.detect(video);
                for (const code of codes) {
                    if (await handleCode(code.rawValue)) return;
                }
            } catch (_) {}
            requestAnimationFrame(tick);
        }
        requestAnimationFrame(tick);
    }

    // ── Fallback scanner (file input + jsqr — iOS Safari) ────────────────────
    // <input type="file" capture="environment"> opens the device camera directly
    // on iOS and returns the captured photo. jsqr decodes the QR from the image.
    function startFallbackScanner() {
        video.style.display = 'none';
        document.querySelector('.viewfinder').style.display = 'none';
        statusEl.textContent = '';

        const wrap = document.querySelector('.video-wrap');

        const fileInput   = document.createElement('input');
        fileInput.type    = 'file';
        fileInput.accept  = 'image/*';
        fileInput.capture = 'environment';
        fileInput.id      = 'qr-file';
        fileInput.style.display = 'none';
        wrap.appendChild(fileInput);

        const btn = document.createElement('button');
        btn.textContent   = '\u{1F4F7}  scan QR code';
        btn.style.cssText =
            'position:absolute;inset:0;width:100%;background:#1C1C1E;' +
            'color:#F7F5F2;border:none;border-radius:14px;font-size:.95rem;cursor:pointer';
        btn.addEventListener('click', () => fileInput.click());
        wrap.appendChild(btn);

        fileInput.addEventListener('change', async (e) => {
            const file = e.target.files[0];
            if (!file) return;
            statusEl.textContent = 'decoding…';
            try {
                const raw = await decodeQrFromImage(file);
                if (!await handleCode(raw)) {
                    statusEl.textContent = 'no matching QR found — tap to try again';
                    e.target.value = '';
                }
            } catch (_) {
                statusEl.textContent = 'could not read QR — tap to try again';
                e.target.value = '';
            }
        });
    }

    // Renders the captured image to a canvas and decodes with jsqr.
    // jsQR is the global exported by the jsqr UMD bundle in <head>.
    function decodeQrFromImage(file) {
        return new Promise((resolve, reject) => {
            const img = new Image();
            const url = URL.createObjectURL(file);
            img.onload = () => {
                URL.revokeObjectURL(url);
                const canvas  = document.createElement('canvas');
                canvas.width  = img.naturalWidth;
                canvas.height = img.naturalHeight;
                const ctx     = canvas.getContext('2d');
                ctx.drawImage(img, 0, 0);
                const pixels  = ctx.getImageData(0, 0, canvas.width, canvas.height);
                const code    = jsQR(pixels.data, pixels.width, pixels.height);
                if (code) { resolve(code.data); }
                else      { reject(new Error('no QR')); }
            };
            img.onerror = () => { URL.revokeObjectURL(url); reject(new Error('load failed')); };
            img.src = url;
        });
    }

    // Shared QR URL handler — called by both scanner paths.
    // Returns true if the code matched the current mode, false if not.
    async function handleCode(rawValue) {
        let url;
        try { url = new URL(rawValue); } catch { return false; }

        if (mode === 'stamp') {
            const t = url.searchParams.get('t');
            const b = parseInt(url.searchParams.get('b'));
            if (t && b === businessId) {
                scanning = false;
                await doStamp(t);
                return true;
            }
        } else {
            const match = url.pathname.match(/^\/nfc\/([a-f0-9-]{36})$/i);
            if (match) {
                scanning = false;
                await doActivate(match[1]);
                return true;
            }
        }
        return false;
    }

    async function doStamp(token) {
        statusEl.textContent = 'recording...';
        try {
            const res  = await fetch('/staff/stamp', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ qr_token: token, business_id: businessId }),
            });
            const data = await res.json();
            if (res.ok) {
                showResult('stamp', true, data.customer_name, data.new_balance, data.reward_available, data.reward_description);
            } else {
                showResult('stamp', false, null, null, false, data.error || 'stamp failed');
            }
        } catch (e) {
            showResult('stamp', false, null, null, false, 'network error');
        }
    }

    async function doActivate(uuid) {
        statusEl.textContent = 'activating...';
        try {
            const res  = await fetch('/api/staff/nfc/activate', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                // Cookie carries the staff JWT — no Authorization header needed
                credentials: 'same-origin',
                body: JSON.stringify({ sticker_uuid: uuid }),
            });
            const data = await res.json();
            if (res.ok) {
                showActivated(uuid);
            } else {
                showResult('activate', false, null, null, false, data.message || 'activation failed');
            }
        } catch (e) {
            showResult('activate', false, null, null, false, 'network error');
        }
    }

    function showActivated(uuid) {
        if (navigator.vibrate) navigator.vibrate([50, 50, 50]);
        card.innerHTML = `
            <div class="result ok">
                <div class="icon">\u{1F4E1}</div>
                <p class="name">cup activated</p>
                <p style="font-size:.75rem;color:#8E8E93;margin-top:8px;word-break:break-all">
                    ${uuid.slice(0, 8)}...
                </p>
                <p style="font-size:.8rem;color:#8E8E93;margin-top:4px">active for 2 hours</p>
            </div>`;
        setTimeout(() => location.reload(), 2000);
    }

    function showResult(ctx, ok, name, balance, rewardAvailable, msg) {
        card.innerHTML = ok
            ? `<div class="result ok">
                   <div class="icon">✓</div>
                   <p class="name">${name}</p>
                   <p class="balance">${balance} steep${balance === 1 ? '' : 's'}</p>
                   ${rewardAvailable ? `<p class="reward">\u{1F381} reward available: ${msg}</p>` : ''}
               </div>`
            : `<div class="result err">
                   <div class="icon">✗</div>
                   <p class="balance">${msg}</p>
               </div>`;
        if (ok && navigator.vibrate) navigator.vibrate(100);
        setTimeout(() => location.reload(), 2500);
    }
})();
