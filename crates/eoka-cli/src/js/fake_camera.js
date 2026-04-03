(function() {
  const videoData = "/*VIDEO_DATA*/";
  const mimeType = "/*MIME_TYPE*/";
  const shouldLoop = /*LOOP*/;

  if (!navigator.mediaDevices) return;
  const origGetUserMedia = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);

  navigator.mediaDevices.getUserMedia = async function(constraints) {
    if (!constraints || !constraints.video) {
      return origGetUserMedia(constraints);
    }

    const video = document.createElement('video');
    video.src = 'data:' + mimeType + ';base64,' + videoData;
    video.loop = shouldLoop;
    video.muted = true;
    video.playsInline = true;
    video.crossOrigin = 'anonymous';

    await new Promise(function(resolve, reject) {
      video.onloadeddata = resolve;
      video.onerror = function() { reject(new Error('Failed to load fake camera video')); };
      video.load();
    });
    await video.play();

    var canvas = document.createElement('canvas');
    canvas.width = video.videoWidth || 640;
    canvas.height = video.videoHeight || 480;
    var ctx = canvas.getContext('2d');

    function draw() {
      if (!video.paused && !video.ended) {
        ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
      }
      requestAnimationFrame(draw);
    }
    requestAnimationFrame(draw);

    var stream = canvas.captureStream(30);

    if (constraints.audio) {
      try {
        var audioCtx = new (window.AudioContext || window.webkitAudioContext)();
        var dest = audioCtx.createMediaStreamDestination();
        stream.addTrack(dest.stream.getAudioTracks()[0]);
      } catch(e) {}
    }

    return stream;
  };

  // Also override enumerateDevices to report a fake camera
  var origEnumerate = navigator.mediaDevices.enumerateDevices.bind(navigator.mediaDevices);
  navigator.mediaDevices.enumerateDevices = async function() {
    var devices = await origEnumerate();
    var hasVideo = devices.some(function(d) { return d.kind === 'videoinput'; });
    if (!hasVideo) {
      devices.push({
        deviceId: 'eoka-fake-camera',
        kind: 'videoinput',
        label: 'Eoka Fake Camera',
        groupId: 'eoka',
        toJSON: function() { return this; }
      });
    }
    return devices;
  };

  console.log('[eoka] fake camera injected (' + (videoData.length / 1024 | 0) + 'KB, loop=' + shouldLoop + ')');
})();
