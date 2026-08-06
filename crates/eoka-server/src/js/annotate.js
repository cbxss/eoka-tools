(() => {
    const data = /*ELEM_DATA*/;
    const container = document.createElement('div');
    container.id = '__eoka_overlay';

    const style = document.createElement('style');
    style.textContent = `
        .__eoka_label {
            position: fixed;
            z-index: 2147483647;
            background: rgba(220, 38, 38, 0.9);
            color: white;
            font: bold 10px/12px monospace;
            padding: 1px 3px;
            border-radius: 2px;
            pointer-events: none;
            white-space: nowrap;
        }
        .__eoka_box {
            position: fixed;
            z-index: 2147483646;
            border: 1.5px solid rgba(220, 38, 38, 0.7);
            pointer-events: none;
            border-radius: 1px;
        }
    `;
    container.appendChild(style);

    // Track label positions to avoid overlaps
    const placed = [];

    for (const el of data) {
        // Border
        const box = document.createElement('div');
        box.className = '__eoka_box';
        box.style.cssText = 'left:' + el.x + 'px;top:' + el.y + 'px;width:' + el.w + 'px;height:' + el.h + 'px';
        container.appendChild(box);

        // Label — try top-left, top-right, bottom-left, inside top-left
        const labelW = String(el.i).length * 7 + 8;
        const labelH = 14;
        const vw = window.innerWidth, vh = window.innerHeight;
        const clampX = v => Math.max(0, Math.min(v, vw - labelW));
        const clampY = v => Math.max(0, Math.min(v, vh - labelH));
        const candidates = [
            [clampX(el.x), clampY(el.y - labelH - 1)],
            [clampX(el.x + el.w - labelW), clampY(el.y - labelH - 1)],
            [clampX(el.x), clampY(el.y + el.h + 1)],
            [clampX(el.x + 2), clampY(el.y + 2)],
        ];

        let bestX = candidates[0][0], bestY = candidates[0][1];
        for (const [cx, cy] of candidates) {
            let overlaps = false;
            for (const p of placed) {
                if (cx < p[0] + p[2] && cx + labelW > p[0] && cy < p[1] + p[3] && cy + labelH > p[1]) {
                    overlaps = true;
                    break;
                }
            }
            if (!overlaps) {
                bestX = cx;
                bestY = cy;
                break;
            }
        }

        placed.push([bestX, bestY, labelW, labelH]);

        const label = document.createElement('div');
        label.className = '__eoka_label';
        label.style.cssText = 'left:' + bestX + 'px;top:' + bestY + 'px;text-shadow:0 0 2px rgba(0,0,0,0.8)';
        label.textContent = String(el.i);
        container.appendChild(label);
    }

    document.body.appendChild(container);
})()
