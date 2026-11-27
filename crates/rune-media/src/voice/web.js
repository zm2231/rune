const PROCESSOR = `
class RuneVoice extends AudioWorkletProcessor {
    constructor(options) {
        super();
        this.block = options.processorOptions.block;
        this.limit = this.block * 8;
        this.batch = new Float32Array(this.block);
        this.filled = 0;
        this.queue = [];
        this.head = 0;
        this.queued = 0;
        this.playing = false;
        this.played = false;
        this.late = 0;
        this.port.onmessage = (event) => {
            this.queue.push(event.data);
            this.queued += event.data.length;
            while (this.queued > this.limit) {
                const chunk = this.queue[0];
                const drop = Math.min(chunk.length - this.head, this.queued - this.limit);
                this.head += drop;
                this.queued -= drop;
                this.late += drop;
                if (this.head === chunk.length) {
                    this.queue.shift();
                    this.head = 0;
                }
            }
        };
    }

    process(inputs, outputs) {
        const input = inputs[0] && inputs[0][0];
        const out = outputs[0][0];
        for (let i = 0; i < out.length; i++) {
            this.batch[this.filled++] = input ? input[i] : 0;
            if (this.filled === this.block) {
                this.port.postMessage({ mic: this.batch, late: this.late }, [this.batch.buffer]);
                this.late = 0;
                this.batch = new Float32Array(this.block);
                this.filled = 0;
            }
        }
        if (!this.playing && this.queued >= this.block * 2) {
            this.playing = true;
            this.played = true;
        }
        for (let i = 0; i < out.length; i++) {
            if (this.playing && this.queued > 0) {
                const chunk = this.queue[0];
                out[i] = chunk[this.head++];
                this.queued--;
                if (this.head === chunk.length) {
                    this.queue.shift();
                    this.head = 0;
                }
            } else {
                out[i] = 0;
                if (this.played) this.late++;
            }
        }
        if (this.queued === 0) this.playing = false;
        return true;
    }
}
registerProcessor('rune-voice', RuneVoice);
`;

let permissionState = 'prompt';
let queried = false;

function devices() {
    return typeof navigator !== 'undefined' ? navigator.mediaDevices : undefined;
}

export function voiceSupported() {
    return !!(devices() && devices().getUserMedia && typeof AudioWorkletNode !== 'undefined');
}

export function voicePermission() {
    if (!voiceSupported()) return 'restricted';
    if (!queried && navigator.permissions && navigator.permissions.query) {
        queried = true;
        navigator.permissions.query({ name: 'microphone' }).then((status) => {
            permissionState = status.state;
            status.onchange = () => { permissionState = status.state; };
        }, () => {});
    }
    return permissionState;
}

export function voiceRequestPermission(done) {
    if (!voiceSupported()) {
        done('restricted');
        return;
    }
    devices().getUserMedia({ audio: true }).then((stream) => {
        stream.getTracks().forEach((track) => track.stop());
        permissionState = 'granted';
        done('granted');
    }, (error) => {
        const denied = error && (error.name === 'NotAllowedError' || error.name === 'SecurityError');
        if (denied) permissionState = 'denied';
        done(denied ? 'denied' : permissionState);
    });
}

export function voiceWakeAfter(ms, callback) {
    setTimeout(callback, Math.max(0, ms));
}

function resumeOnGesture(context) {
    if (context.state !== 'suspended') return;
    const resume = () => {
        context.resume();
        removeEventListener('pointerdown', resume, true);
        removeEventListener('keydown', resume, true);
    };
    addEventListener('pointerdown', resume, true);
    addEventListener('keydown', resume, true);
}

export function voiceOpen(rate, frames, ended) {
    const session = { closed: false, context: null, stream: null, node: null, source: null, onDevices: null };
    const report = (kind, message) => {
        if (!session.closed) ended(kind, message);
    };
    session.report = report;
    (async () => {
        try {
            const context = new AudioContext({ sampleRate: rate, latencyHint: 'interactive' });
            session.context = context;
            resumeOnGesture(context);
            const url = URL.createObjectURL(new Blob([PROCESSOR], { type: 'text/javascript' }));
            try {
                await context.audioWorklet.addModule(url);
            } finally {
                URL.revokeObjectURL(url);
            }
            const stream = await devices().getUserMedia({
                audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true, channelCount: 1 },
            });
            if (session.closed) {
                stream.getTracks().forEach((track) => track.stop());
                return;
            }
            permissionState = 'granted';
            session.stream = stream;
            const block = Math.floor(rate / 100);
            const node = new AudioWorkletNode(context, 'rune-voice', {
                numberOfInputs: 1,
                numberOfOutputs: 1,
                outputChannelCount: [1],
                channelCount: 1,
                channelCountMode: 'explicit',
                processorOptions: { block },
            });
            node.port.onmessage = (event) => {
                if (session.closed) return;
                const out = frames(event.data.mic, event.data.late);
                node.port.postMessage(out, [out.buffer]);
            };
            const source = context.createMediaStreamSource(stream);
            source.connect(node);
            node.connect(context.destination);
            session.node = node;
            session.source = source;
            stream.getAudioTracks().forEach((track) => {
                track.addEventListener('ended', () => report('lost', 'the microphone went away'));
            });
            session.onDevices = () => report('changed', '');
            devices().addEventListener('devicechange', session.onDevices);
        } catch (error) {
            const name = error && error.name;
            if (name === 'NotAllowedError' || name === 'SecurityError') {
                permissionState = 'denied';
                report('denied', 'microphone access was refused; allow it in the browser\'s site settings');
            } else {
                report(name === 'NotSupportedError' ? 'unsupported' : 'failed', String((error && error.message) || error));
            }
        }
    })();
    return session;
}

export function voiceClose(session) {
    session.closed = true;
    if (session.onDevices) devices().removeEventListener('devicechange', session.onDevices);
    if (session.node) {
        session.node.port.onmessage = null;
        session.node.disconnect();
    }
    if (session.source) session.source.disconnect();
    if (session.stream) session.stream.getTracks().forEach((track) => track.stop());
    if (session.context) session.context.close().catch(() => {});
}
