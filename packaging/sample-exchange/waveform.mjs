export function waveform(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const tag = i => String.fromCharCode(...bytes.subarray(i,i+4));
  if (bytes.length < 44 || tag(0)!=="RIFF" || tag(8)!=="WAVE") throw Error("Invalid WAV");
  let format, channels, bits, stride, offset, size;
  for (let i=12; i+8<=bytes.length;) {
    const len=view.getUint32(i+4,true), start=i+8;
    if (start+len>bytes.length) throw Error("Incomplete WAV");
    if (tag(i)==="fmt " && len>=16) {
      format=view.getUint16(start,true); channels=view.getUint16(start+2,true);
      stride=view.getUint16(start+12,true); bits=view.getUint16(start+14,true);
      if (format===65534 && len>=40) format=view.getUint16(start+24,true);
    }
    if (tag(i)==="data") {offset=start;size=len;}
    i=start+len+(len%2);
  }
  if (!channels || !size || stride!==channels*bits/8 ||
      !(format===1 && [8,16,24,32].includes(bits) || format===3 && [32,64].includes(bits)))
    throw Error("Choose a PCM or floating-point WAV");
  const frames=Math.floor(size/stride), peaks=Array(64).fill(0);
  if (!frames) throw Error("Empty WAV");
  for (let frame=0;frame<frames;frame++) {
    const bin=Math.floor(frame*64/frames);
    for(let c=0;c<channels;c++){
      const p=offset+frame*stride+c*bits/8;
      let v;
      if(format===3) v=bits===32?view.getFloat32(p,true):view.getFloat64(p,true);
      else if(bits===8) v=(bytes[p]-128)/128;
      else if(bits===16) v=view.getInt16(p,true)/32768;
      else if(bits===24) v=((bytes[p]<<8|bytes[p+1]<<16|bytes[p+2]<<24)>>8)/8388608;
      else v=view.getInt32(p,true)/2147483648;
      if(Number.isFinite(v)) peaks[bin]=Math.max(peaks[bin],Math.min(1,Math.abs(v)));
    }
  }
  const max=Math.max(...peaks)||1;
  return peaks.map(v=>Math.round(v/max*255));
}
