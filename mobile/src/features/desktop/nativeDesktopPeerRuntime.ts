export const nativeDesktopPeerRuntime = String.raw`
    let offerSent = false;
    const pendingLocalCandidates = [];
    const pendingRemoteCandidates = [];

    const advertiseH264ReceiveLevel = (sdp, profileLevelId) => {
      const requested = String(profileLevelId || '').toLowerCase();
      if (!/^[0-9a-f]{6}$/.test(requested)) return sdp;
      const requestedLevel = Number.parseInt(requested.slice(4), 16);
      const lines = sdp.split(/\r?\n/);
      const h264PayloadTypes = new Set(
        lines
          .map((line) => {
            const match = line.match(/^a=rtpmap:(\d+)\s+H264\/90000/i);
            return match ? match[1] : null;
          })
          .filter(Boolean),
      );
      return lines
        .map((line) => {
          const match = line.match(/^a=fmtp:(\d+)\s+(.+)$/i);
          if (!match || !h264PayloadTypes.has(match[1])) return line;
          const profileMatch = match[2].match(
            /(?:^|;)\s*profile-level-id=([0-9a-f]{6})/i,
          );
          const profile = profileMatch ? profileMatch[1] : '';
          if (!profile || Number.parseInt(profile.slice(4), 16) >= requestedLevel) return line;
          const maxReceiveLevel = profile.slice(2, 4) + requested.slice(4);
          if (/(?:^|;)\s*max-recv-level=[0-9a-f]+/i.test(match[2])) {
            return line.replace(
              /max-recv-level=[0-9a-f]+/i,
              'max-recv-level=' + maxReceiveLevel,
            );
          }
          return line + ';max-recv-level=' + maxReceiveLevel;
        })
        .join('\r\n');
    };

    const closePeerConnection = () => {
      const currentPeer = peerConnection;
      peerConnection = null;
      controlChannel = null;
      pointerChannel = null;
      if (videoFrameCallback !== null) {
        if (typeof video.cancelVideoFrameCallback === 'function') {
          video.cancelVideoFrameCallback(videoFrameCallback);
        } else {
          window.cancelAnimationFrame(videoFrameCallback);
        }
      }
      videoFrameCallback = null;
      video.srcObject = null;
      offerSent = false;
      pendingLocalCandidates.length = 0;
      pendingRemoteCandidates.length = 0;
      currentPeer?.close();
    };

    const startPeerConnection = async (iceServers, h264ProfileLevelId) => {
      if (peerConnection) return;
      if (typeof RTCPeerConnection !== 'function') {
        throw new Error('This device does not support WebRTC');
      }

      const peer = new RTCPeerConnection({
        iceServers: Array.isArray(iceServers) ? iceServers : [],
      });
      peerConnection = peer;
      const channel = peer.createDataChannel('latitude-control', { ordered: true });
      controlChannel = channel;
      const pointer = peer.createDataChannel('latitude-pointer', {
        ordered: false,
        maxRetransmits: 0,
      });
      pointerChannel = pointer;
      pointer.onclose = () => {
        if (pointerChannel === pointer) pointerChannel = null;
      };
      channel.onopen = () => {
        if (controlChannel !== channel) return;
        reconnectDelay = 1000;
        updateNativeState({ connected: true });
        setConnectedStatus();
      };
      channel.onmessage = handleControlMessage;
      channel.onclose = () => {
        if (controlChannel === channel) controlChannel = null;
      };
      channel.onerror = () => setStatus('Desktop control channel failed', true);

      peer.ontrack = (event) => {
        if (peerConnection !== peer || event.track.kind !== 'video') return;
        try {
          if ('playoutDelayHint' in event.receiver) {
            event.receiver.playoutDelayHint = 0;
          }
          if ('jitterBufferTarget' in event.receiver) {
            event.receiver.jitterBufferTarget = 0;
          }
        } catch (_) {
          // These low-latency hints are optional and browser-specific.
        }
        video.srcObject = event.streams[0] || new MediaStream([event.track]);
        Promise.resolve(video.play())
          .then(scheduleVideoFrame)
          .catch((error) => setStatus(error?.message || 'Desktop video could not start', true));
      };
      peer.onconnectionstatechange = () => {
        if (peerConnection !== peer) return;
        if (peer.connectionState === 'connected') {
          updateNativeState({ connected: true });
          setConnectedStatus();
        } else if (peer.connectionState === 'connecting') {
          setStatus('Connecting media');
        } else if (peer.connectionState === 'failed') {
          updateNativeState({ connected: false });
          setStatus('WebRTC connection failed', true);
          socket?.close();
        }
      };
      peer.onicecandidate = (event) => {
        if (peerConnection !== peer || !event.candidate) return;
        const candidate = event.candidate.toJSON();
        if (offerSent) {
          sendSignal({ type: 'candidate', candidate });
        } else {
          pendingLocalCandidates.push(candidate);
        }
      };

      peer.addTransceiver('video', { direction: 'recvonly' });
      const offer = await peer.createOffer();
      await peer.setLocalDescription({
        type: offer.type,
        sdp: advertiseH264ReceiveLevel(offer.sdp || '', h264ProfileLevelId),
      });
      if (peerConnection !== peer || !peer.localDescription) return;
      setStatus('Negotiating');
      if (sendSignal({ type: 'offer', sdp: peer.localDescription.sdp })) {
        offerSent = true;
        for (const candidate of pendingLocalCandidates.splice(0)) {
          sendSignal({ type: 'candidate', candidate });
        }
      }
    };

    const connect = () => {
      if (socket) return;
      clearReconnectTimer();
      setStatus('Connecting');
      const nextSocket = new WebSocket(websocketUrl);
      socket = nextSocket;
      nextSocket.onopen = () => {
        if (socket !== nextSocket) return;
        reconnectDelay = 1000;
        updateNativeState({ connected: false });
        setStatus('Negotiating');
      };
      nextSocket.onmessage = async (event) => {
        if (socket !== nextSocket) return;
        if (typeof event.data !== 'string') return;
        let message;
        try {
          message = JSON.parse(event.data);
        } catch (_) {
          return;
        }
        if (message.type === 'hello') {
          updateGeometry(message);
          try {
            await startPeerConnection(message.ice_servers, message.h264_profile_level_id);
          } catch (error) {
            setStatus(error?.message || 'WebRTC could not be started', true);
            nextSocket.close();
          }
        } else if (message.type === 'answer') {
          if (!peerConnection) return;
          try {
            await peerConnection.setRemoteDescription({ type: 'answer', sdp: message.sdp });
            for (const candidate of pendingRemoteCandidates.splice(0)) {
              await peerConnection.addIceCandidate(candidate);
            }
            setStatus('Connecting media');
          } catch (error) {
            setStatus(error?.message || 'WebRTC answer was rejected', true);
            nextSocket.close();
          }
        } else if (message.type === 'candidate') {
          if (!peerConnection || !message.candidate) return;
          try {
            if (peerConnection.remoteDescription) {
              await peerConnection.addIceCandidate(message.candidate);
            } else {
              pendingRemoteCandidates.push(message.candidate);
            }
          } catch (error) {
            setStatus(error?.message || 'WebRTC ICE candidate was rejected', true);
            nextSocket.close();
          }
        } else if (message.type === 'error') {
          setStatus(message.message || 'Desktop connection failed', true);
        }
      };
      nextSocket.onerror = () => {
        if (socket === nextSocket) setStatus('Desktop connection failed', true);
      };
      nextSocket.onclose = () => {
        if (socket !== nextSocket) return;
        socket = null;
        pressedModifiers.clear();
        controlGranted = false;
        updateModifiers();
        updateNativeState({ controlGranted });
        closePeerConnection();
        updateNativeState({ connected: false });
        scheduleReconnect();
      };
    };
`;
