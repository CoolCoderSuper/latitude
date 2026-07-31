const CONTROL_CHANNEL_LABEL = 'latitude-control';
const POINTER_CHANNEL_LABEL = 'latitude-pointer';
const MAX_POINTER_BUFFER_BYTES = 256;

export class DesktopPeer {
  constructor({
    onControlOpen,
    onControlMessage,
    onControlError,
    onTrack,
    onConnectionState,
    onIceCandidate,
  }) {
    this.onControlOpen = onControlOpen;
    this.onControlMessage = onControlMessage;
    this.onControlError = onControlError;
    this.onTrack = onTrack;
    this.onConnectionState = onConnectionState;
    this.onIceCandidate = onIceCandidate;
    this.peer = null;
    this.controlChannel = null;
    this.pointerChannel = null;
    this.offerSent = false;
    this.pendingLocalCandidates = [];
    this.pendingRemoteCandidates = [];
  }

  async start(iceServers, h264ProfileLevelId) {
    if (this.peer) {
      return null;
    }
    if (typeof RTCPeerConnection !== 'function') {
      throw new Error('This browser does not support WebRTC');
    }

    const peer = new RTCPeerConnection({
      iceServers: Array.isArray(iceServers) ? iceServers : [],
    });
    this.peer = peer;
    this.installDataChannels(peer);

    peer.addEventListener('track', (event) => {
      if (this.peer !== peer || event.track.kind !== 'video') return;
      applyLowLatencyHints(event.receiver);
      this.onTrack(event);
    });
    peer.addEventListener('connectionstatechange', () => {
      if (this.peer === peer) {
        this.onConnectionState(peer.connectionState);
      }
    });
    peer.addEventListener('icecandidate', (event) => {
      if (this.peer !== peer || !event.candidate) return;
      const candidate = event.candidate.toJSON();
      if (this.offerSent) {
        this.onIceCandidate(candidate);
      } else {
        this.pendingLocalCandidates.push(candidate);
      }
    });

    peer.addTransceiver('video', { direction: 'recvonly' });
    const offer = await peer.createOffer();
    await peer.setLocalDescription({
      type: offer.type,
      sdp: advertiseH264ReceiveLevel(offer.sdp || '', h264ProfileLevelId),
    });
    return peer.localDescription?.sdp || '';
  }

  releaseIceCandidates() {
    if (!this.peer || this.offerSent) return;
    this.offerSent = true;
    for (const candidate of this.pendingLocalCandidates.splice(0)) {
      this.onIceCandidate(candidate);
    }
  }

  sendControl(command) {
    if (!this.controlChannel || this.controlChannel.readyState !== 'open') {
      return false;
    }
    this.controlChannel.send(JSON.stringify(command));
    return true;
  }

  async acceptAnswer(sdp) {
    if (!this.peer) {
      return false;
    }
    await this.peer.setRemoteDescription({ type: 'answer', sdp });
    for (const candidate of this.pendingRemoteCandidates.splice(0)) {
      await this.peer.addIceCandidate(candidate);
    }
    return true;
  }

  async addCandidate(candidate) {
    if (!this.peer) {
      return false;
    }
    if (!this.peer.remoteDescription) {
      this.pendingRemoteCandidates.push(candidate);
      return true;
    }
    await this.peer.addIceCandidate(candidate);
    return true;
  }

  sendPointer(command) {
    if (this.pointerChannel && this.pointerChannel.readyState === 'open') {
      if (this.pointerChannel.bufferedAmount >= MAX_POINTER_BUFFER_BYTES) {
        return false;
      }
      this.pointerChannel.send(JSON.stringify(command));
      return true;
    }
    return this.sendControl(command);
  }

  close() {
    const peer = this.peer;
    this.peer = null;
    this.controlChannel = null;
    this.pointerChannel = null;
    this.offerSent = false;
    this.pendingLocalCandidates = [];
    this.pendingRemoteCandidates = [];
    peer?.close();
  }

  installDataChannels(peer) {
    const control = peer.createDataChannel(CONTROL_CHANNEL_LABEL, { ordered: true });
    this.controlChannel = control;
    control.addEventListener('open', () => {
      if (this.controlChannel === control) {
        this.onControlOpen();
      }
    });
    control.addEventListener('message', this.onControlMessage);
    control.addEventListener('close', () => {
      if (this.controlChannel === control) {
        this.controlChannel = null;
      }
    });
    control.addEventListener('error', this.onControlError);

    const pointer = peer.createDataChannel(POINTER_CHANNEL_LABEL, {
      ordered: false,
      maxRetransmits: 0,
    });
    this.pointerChannel = pointer;
    pointer.addEventListener('close', () => {
      if (this.pointerChannel === pointer) {
        this.pointerChannel = null;
      }
    });
  }
}

function advertiseH264ReceiveLevel(sdp, profileLevelId) {
  const requested = String(profileLevelId || '').toLowerCase();
  if (!/^[0-9a-f]{6}$/.test(requested)) return sdp;
  const requestedLevel = Number.parseInt(requested.slice(4), 16);
  const lines = sdp.split(/\r?\n/);
  const h264PayloadTypes = new Set(
    lines
      .map((line) => line.match(/^a=rtpmap:(\d+)\s+H264\/90000/i)?.[1])
      .filter(Boolean),
  );
  const updated = lines.map((line) => {
    const match = line.match(/^a=fmtp:(\d+)\s+(.+)$/i);
    if (!match || !h264PayloadTypes.has(match[1])) return line;
    const profile = match[2].match(/(?:^|;)\s*profile-level-id=([0-9a-f]{6})/i)?.[1];
    if (!profile || Number.parseInt(profile.slice(4), 16) >= requestedLevel) return line;
    const maxReceiveLevel = `${profile.slice(2, 4)}${requested.slice(4)}`;
    if (/(?:^|;)\s*max-recv-level=[0-9a-f]+/i.test(match[2])) {
      return line.replace(
        /max-recv-level=[0-9a-f]+/i,
        `max-recv-level=${maxReceiveLevel}`,
      );
    }
    return `${line};max-recv-level=${maxReceiveLevel}`;
  });
  return updated.join('\r\n');
}

function applyLowLatencyHints(receiver) {
  try {
    if ('playoutDelayHint' in receiver) {
      receiver.playoutDelayHint = 0;
    }
    if ('jitterBufferTarget' in receiver) {
      receiver.jitterBufferTarget = 0;
    }
  } catch (_) {
    // These low-latency hints are optional and browser-specific.
  }
}
