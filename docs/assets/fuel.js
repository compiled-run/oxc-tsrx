// Decorative rocket-fuel plume. Never logs: on failure the CSS fill shows.

const VERT = 'attribute vec2 a;void main(){gl_Position=vec4(a,0.,1.);}'

const FRAG = `precision mediump float;
uniform vec2 uR;uniform float uT;uniform float uX;uniform float uC;uniform float uL;
float h(vec2 p){return fract(sin(dot(p,vec2(41.3,289.1)))*43758.5453);}
float vn(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*f*(f*(f*6.-15.)+10.);
return mix(mix(h(i),h(i+vec2(1,0)),f.x),mix(h(i+vec2(0,1)),h(i+vec2(1,1)),f.x),f.y);}
float fbm(vec2 p){float s=0.,a=.5;for(int k=0;k<4;k++){s+=a*vn(p);p*=2.03;a*=.5;}return s;}
vec3 ramp(float t){vec3 c=mix(vec3(.102),vec3(.93),uL);
c=mix(c,mix(mix(vec3(.914,.071,.659),vec3(.81,0.,.57),uL),mix(vec3(.204,.180,.475),vec3(.17,.18,.47),uL),uC),smoothstep(0.,.06,t));
c=mix(c,mix(mix(vec3(1.,.227,.369),vec3(.93,0.,.24),uL),mix(vec3(.188,.361,.702),vec3(.17,.25,.59),uL),uC),smoothstep(.06,.15,t));
c=mix(c,mix(mix(vec3(1.,.478,.102),vec3(.86,.29,0.),uL),mix(vec3(.200,.541,.800),vec3(.14,.31,.68),uL),uC),smoothstep(.15,.30,t));
c=mix(c,mix(mix(vec3(1.,.655,.149),vec3(.84,.39,0.),uL),mix(vec3(.243,.702,.769),vec3(.09,.40,.74),uL),uC),smoothstep(.30,.55,t));
c=mix(c,mix(mix(vec3(1.,.878,.4),vec3(.78,.51,0.),uL),mix(vec3(.565,.851,.886),vec3(.06,.50,.71),uL),uC),smoothstep(.55,.78,t));
c=mix(c,mix(mix(vec3(1.,.984,.91),vec3(.73,.61,0.),uL),mix(vec3(.878,.953,.973),vec3(.05,.58,.64),uL),uC),smoothstep(.78,1.,t));return c;}
void main(){vec2 p=gl_FragCoord.xy/uR.y*2.4;
float q=fbm(p+vec2(-uT*.7,uT*.55));
float r=fbm(p+q*1.2+vec2(-uT*1.1,uT*.8));
float n=fbm(p+r*1.5+vec2(-uT*1.5,uT*.4));
float x=gl_FragCoord.x/uX-(r-.45)*.95;
float e=1.-x*.5;
float heat=clamp(e*(.45+1.28*n)-.06,0.,1.)+.35*exp(-x/.22);
heat*=.84+.125*p.y;
float v=abs(p.y/1.2-1.);
float a=max(clamp(heat*7.,0.,1.),smoothstep(0.,.34,e)*.88);
a*=1.-.5*smoothstep(.58,1.,v)*smoothstep(.2,1.4,x);
float d=1.-.22*uC;
gl_FragColor=vec4(ramp(clamp(heat,0.,1.))*a*d,a*mix(1.,d,uL));}`

// The dark stops end in near-white, invisible on the light track. uL swaps in
// a darkened light ramp instead, the same fix the gate meters already ship.
const lit = () => (document.documentElement.classList.contains('dark') ? 0 : 1)

// The dark field dissolves into the track, so the canvas is transparent and the
// shader writes premultiplied colour to match premultipliedAlpha.
// prettier-ignore
const CTX = { alpha: true, premultipliedAlpha: true, antialias: false, depth: false, stencil: false, powerPreference: 'low-power' }

function shader(gl, type, src) {
  const s = gl.createShader(type)
  gl.shaderSource(s, src)
  gl.compileShader(s)
  return gl.getShaderParameter(s, gl.COMPILE_STATUS) ? s : null
}

function build(row, dpr) {
  const track = row.querySelector('.comp-track')
  const fill = row.querySelector('.comp-fill')
  if (!track || !fill) return null
  const canvas = document.createElement('canvas')
  canvas.setAttribute('aria-hidden', 'true')
  const gl = canvas.getContext('webgl', CTX)
  if (!gl) return null
  const vs = shader(gl, gl.VERTEX_SHADER, VERT)
  const fs = shader(gl, gl.FRAGMENT_SHADER, FRAG)
  if (!vs || !fs) return null
  const program = gl.createProgram()
  gl.attachShader(program, vs)
  gl.attachShader(program, fs)
  gl.linkProgram(program)
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) return null
  gl.useProgram(program)
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer())
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
  const attr = gl.getAttribLocation(program, 'a')
  gl.enableVertexAttribArray(attr)
  gl.vertexAttribPointer(attr, 2, gl.FLOAT, false, 0, 0)
  // The mixed lane is the same fire after it burned out: cool palette, slower.
  const cool = row.dataset.key === 'oxcTsrxMixed'
  const u = (n) => gl.getUniformLocation(program, n)
  gl.uniform1f(u('uC'), cool ? 1 : 0)
  gl.uniform1f(u('uL'), lit())
  const item = {
    gl,
    canvas,
    track,
    dpr,
    speed: cool ? 0.45 : 1,
    frac: (parseFloat(fill.style.width) || 8) / 100,
    uR: u('uR'),
    uT: u('uT'),
    uX: u('uX'),
    uL: u('uL'),
    w: 0,
    h: 0,
    seen: false,
    dead: false,
  }
  canvas.addEventListener('webglcontextlost', (event) => {
    event.preventDefault()
    item.dead = true
  })
  return item
}

// Pill = the bar's own length plus a dissipation tail, never the whole track.
function resize(item) {
  const box = item.track.getBoundingClientRect()
  const reach = Math.max(box.width * item.frac, 14)
  const wide = Math.min(Math.max(reach * 3.2, reach + 90), 300)
  const w = Math.max(Math.round(wide * item.dpr), 1)
  const h = Math.max(Math.round(box.height * item.dpr), 1)
  if (w === item.w && h === item.h) return false
  item.w = w
  item.h = h
  item.canvas.width = w
  item.canvas.height = h
  item.canvas.style.width = `${wide.toFixed(1)}px`
  item.gl.viewport(0, 0, w, h)
  item.gl.uniform2f(item.uR, w, h)
  item.gl.uniform1f(item.uX, reach * item.dpr)
  return true
}

function paint(item, seconds) {
  if (item.dead) return
  item.gl.uniform1f(item.uT, seconds * item.speed)
  item.gl.drawArrays(item.gl.TRIANGLES, 0, 3)
}

export function init(rows, cleanups) {
  const dpr = Math.min(window.devicePixelRatio || 1, 2)
  const items = []
  for (const row of rows) {
    const item = build(row, dpr)
    if (!item) continue
    resize(item)
    paint(item, 0)
    if (item.dead) continue
    item.track.append(item.canvas)
    items.push(item)
  }
  if (!items.length) return

  const start = performance.now()
  // Wrapped: an unbounded clock eventually costs the hash its precision.
  const clock = () => ((performance.now() - start) / 1000) % 300
  const awake = () => items.some((it) => it.seen)
  let raf = 0

  const frame = () => {
    raf = 0
    const seconds = clock()
    for (const item of items) paint(item, seconds)
    if (awake()) raf = requestAnimationFrame(frame)
  }
  const wake = () => {
    if (!raf && awake()) raf = requestAnimationFrame(frame)
  }
  const io = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      const item = items.find((it) => it.track === entry.target)
      if (item) item.seen = entry.isIntersecting
    }
    wake()
  })
  const ro = new ResizeObserver(() => {
    for (const item of items) if (resize(item) && !raf) paint(item, clock())
  })
  // app.js toggles html.dark with no event, so the class is the only hook.
  const mo = new MutationObserver(() => {
    const light = lit()
    const seconds = clock()
    for (const item of items) {
      if (item.dead) continue
      item.gl.uniform1f(item.uL, light)
      paint(item, seconds)
    }
  })
  mo.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] })
  for (const item of items) {
    io.observe(item.track)
    ro.observe(item.track)
  }

  cleanups.push(() => {
    if (raf) cancelAnimationFrame(raf)
    raf = 0
    io.disconnect()
    ro.disconnect()
    mo.disconnect()
    for (const item of items) {
      item.seen = false
      item.canvas.remove()
    }
  })
}
