const CONTROL_CHANNEL_LABEL = 'latitude-control';
const POINTER_CHANNEL_LABEL = 'latitude-pointer';
const MAX_POINTER_BUFFER_BYTES = 4096;

export class NativeDesktopPeer {
  constructor({ onControlOpen, onControlMessage, onControlError, onTrack, onConnectionState }) {
    this.onControlOpen = onControlOpen;
    this.onControlMessage = onControlMessage;
    this.onControlError = onControlError;
    this.onTrack = onTrack;
    this.onConnectionState = onConnectionState;
    this.peer = null;
    this.controlChannel = null;
    this.pointerChannel = null;
  }

  async start(iceServers) {
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

    peer.addTransceiver('video', { direction: 'recvonly' });
    await peer.setLocalDescription(await peer.createOffer());
    await waitForIceGatheringComplete(peer);
    return peer.localDescription?.sdp || '';
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
    return true;
  }

  sendPointer(command) {
    if (this.pointerChannel && this.pointerChannel.readyState === 'open') {
      if (this.pointerChannel.bufferedAmount < MAX_POINTER_BUFFER_BYTES) {
        this.pointerChannel.send(JSON.stringify(command));
      }
      return true;
    }
    return this.sendControl(command);
  }

  close() {
    const peer = this.peer;
    this.peer = null;
    this.controlChannel = null;
    this.pointerChannel = null;
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

function waitForIceGatheringComplete(peer) {
  if (peer.iceGatheringState === 'complete') {
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    const timeout = window.setTimeout(finish, 10000);
    function finish() {
      window.clearTimeout(timeout);
      peer.removeEventListener('icegatheringstatechange', handleStateChange);
      resolve();
    }
    const handleStateChange = () => {
      if (peer.iceGatheringState === 'complete') {
        finish();
      }
    };
    peer.addEventListener('icegatheringstatechange', handleStateChange);
  });
}
