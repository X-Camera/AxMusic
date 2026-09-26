import { forwardRef, useEffect, useImperativeHandle, useRef, useState } from "react";

import type { VizFrame } from "./effects/types";
import { rgba } from "./effects/types";

/**
 * 极光丝绸：手写 WebGL fbm domain-warping 流光（无依赖）。
 * - canvas context 模式首次创建即锁定，故本组件独占一个 canvas 元素；
 *   mode 切换（WebGL↔2D 回退）通过 key 重挂新元素规避。
 * - 宿主统一驱动 render()（单 rAF 管所有效果，fps cap / hidden 单点生效）。
 * - WebGL 不可用（远程桌面 / GPU 黑名单）或上下文丢失且重建失败 → 永久回退 2D。
 * - dev 钩子：URL 带 ?viz_force_2d=1 强制走 2D 回退路径验证。
 */

export interface SilkHandle {
  render(f: VizFrame): void;
  resize(cssW: number, cssH: number, scale: number): void;
}

const VERT = `
attribute vec2 a_pos;
void main() { gl_Position = vec4(a_pos, 0.0, 1.0); }
`;

const FRAG = `
precision mediump float;
uniform vec2 u_res;
uniform float u_time;
uniform vec3 u_a;
uniform vec3 u_b;
uniform vec3 u_c;
uniform float u_oct;
uniform float u_flow;
uniform float u_bright;
uniform float u_bass;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123); }
float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
             mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}
float fbm(vec2 p) {
  float v = 0.0;
  float amp = 0.5;
  for (int i = 0; i < 6; i++) {
    if (float(i) >= u_oct) break;
    v += amp * noise(p);
    p = p * 2.03 + vec2(17.3, 9.1);
    amp *= 0.5;
  }
  return v;
}
void main() {
  vec2 p = (gl_FragCoord.xy - 0.5 * u_res) / min(u_res.x, u_res.y);
  float t = u_time * (0.06 + u_flow * 0.3);
  vec2 q = vec2(fbm(p * 1.6 + t * 0.8), fbm(p * 1.6 + vec2(5.2, 1.3) - t * 0.6));
  vec2 r = vec2(fbm(p * 1.6 + q * 1.8 + vec2(1.7, 9.2) + t * 0.5),
                fbm(p * 1.6 + q * 1.8 + vec2(8.3, 2.8) - t * 0.4));
  float v = fbm(p * 1.6 + r * 2.2);
  vec3 col = mix(u_a, u_b, clamp(q.x * 1.4 - 0.2, 0.0, 1.0));
  col = mix(col, u_c, clamp(r.y * 1.4 - 0.2, 0.0, 1.0));
  float lum = (0.22 + v * 0.95) * (0.5 + u_bright * 0.8) * (0.82 + u_bass * 0.45);
  // 半透明丝绸：暗处透明露出面板底色，明暗双主题都安全
  float alpha = clamp(lum * 0.85, 0.0, 0.8);
  gl_FragColor = vec4(col * max(lum, 0.12), alpha);
}
`;

interface GlState {
  gl: WebGLRenderingContext;
  prog: WebGLProgram;
  uTime: WebGLUniformLocation | null;
  uRes: WebGLUniformLocation | null;
  uA: WebGLUniformLocation | null;
  uB: WebGLUniformLocation | null;
  uC: WebGLUniformLocation | null;
  uOct: WebGLUniformLocation | null;
  uFlow: WebGLUniformLocation | null;
  uBright: WebGLUniformLocation | null;
  uBass: WebGLUniformLocation | null;
}

function initGl(canvas: HTMLCanvasElement): GlState | null {
  const gl = canvas.getContext("webgl", {
    alpha: true,
    antialias: false,
    depth: false,
    stencil: false,
    premultipliedAlpha: false,
    powerPreference: "low-power",
  });
  if (!gl) return null;
  const compile = (type: number, src: string) => {
    const sh = gl.createShader(type);
    if (!sh) return null;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
      gl.deleteShader(sh);
      return null;
    }
    return sh;
  };
  const vs = compile(gl.VERTEX_SHADER, VERT);
  const fs = compile(gl.FRAGMENT_SHADER, FRAG);
  const prog = gl.createProgram();
  if (!vs || !fs || !prog) return null;
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return null;
  gl.useProgram(prog);

  // fullscreen triangle
  const buf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  const loc = gl.getAttribLocation(prog, "a_pos");
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

  const u = (n: string) => gl.getUniformLocation(prog, n);
  return {
    gl,
    prog,
    uTime: u("u_time"),
    uRes: u("u_res"),
    uA: u("u_a"),
    uB: u("u_b"),
    uC: u("u_c"),
    uOct: u("u_oct"),
    uFlow: u("u_flow"),
    uBright: u("u_bright"),
    uBass: u("u_bass"),
  };
}

export const SilkCanvas = forwardRef<SilkHandle, { opacity: number }>(function SilkCanvas(
  { opacity },
  ref,
) {
  const force2d =
    typeof window !== "undefined" && /[?&]viz_force_2d=1/.test(window.location.search);
  const [mode, setMode] = useState<"webgl" | "2d">(force2d ? "2d" : "webgl");
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const glRef = useRef<GlState | null>(null);
  const ctx2dRef = useRef<CanvasRenderingContext2D | null>(null);
  const sizeRef = useRef({ w: 1, h: 1, scale: 1 });

  // 初始化/重建 GL（mode 变化时 canvas 经 key 重挂，effect 重跑）
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    glRef.current = null;
    ctx2dRef.current = null;
    if (mode === "webgl") {
      const st = initGl(canvas);
      if (st) {
        glRef.current = st;
        const onLost = (e: Event) => e.preventDefault();
        const onRestored = () => {
          glRef.current = initGl(canvas);
          if (!glRef.current) setMode("2d");
        };
        canvas.addEventListener("webglcontextlost", onLost);
        canvas.addEventListener("webglcontextrestored", onRestored);
        return () => {
          canvas.removeEventListener("webglcontextlost", onLost);
          canvas.removeEventListener("webglcontextrestored", onRestored);
        };
      }
      setMode("2d");
      return;
    }
    ctx2dRef.current = canvas.getContext("2d");
  }, [mode]);

  useImperativeHandle(
    ref,
    () => ({
      resize(cssW: number, cssH: number, scale: number) {
        const canvas = canvasRef.current;
        if (!canvas) return;
        const prev = sizeRef.current;
        if (prev.w === cssW && prev.h === cssH && prev.scale === scale) return;
        sizeRef.current = { w: cssW, h: cssH, scale };
        const dpr = Math.min(window.devicePixelRatio || 1, 2) * scale;
        canvas.width = Math.max(1, Math.floor(cssW * dpr));
        canvas.height = Math.max(1, Math.floor(cssH * dpr));
        canvas.style.width = `${cssW}px`;
        canvas.style.height = `${cssH}px`;
        if (glRef.current) {
          glRef.current.gl.viewport(0, 0, canvas.width, canvas.height);
        } else if (ctx2dRef.current) {
          ctx2dRef.current.setTransform(dpr, 0, 0, dpr, 0, 0);
        }
      },
      render(f: VizFrame) {
        const st = glRef.current;
        const sp = f.settings.silk;
        if (st) {
          const { gl } = st;
          gl.useProgram(st.prog);
          gl.uniform1f(st.uTime, f.t);
          gl.uniform2f(st.uRes, gl.drawingBufferWidth, gl.drawingBufferHeight);
          gl.uniform3f(st.uA, f.colors.a[0] / 255, f.colors.a[1] / 255, f.colors.a[2] / 255);
          gl.uniform3f(st.uB, f.colors.b[0] / 255, f.colors.b[1] / 255, f.colors.b[2] / 255);
          gl.uniform3f(st.uC, f.colors.c[0] / 255, f.colors.c[1] / 255, f.colors.c[2] / 255);
          gl.uniform1f(st.uOct, 3 + Math.round(sp.complexity * 3));
          gl.uniform1f(st.uFlow, sp.flow * f.settings.speed);
          gl.uniform1f(st.uBright, sp.brightness * (0.5 + f.settings.intensity * 0.8));
          gl.uniform1f(st.uBass, f.bass);
          gl.clearColor(0, 0, 0, 0);
          gl.clear(gl.COLOR_BUFFER_BIT);
          gl.drawArrays(gl.TRIANGLES, 0, 3);
          return;
        }
        const ctx = ctx2dRef.current;
        if (!ctx) return;
        // 2D 回退：三层旋转流光带 + screen 合成
        const { w, h, t, colors: cols } = f;
        ctx.clearRect(0, 0, w, h);
        ctx.globalCompositeOperation = "screen";
        const palette = [cols.a, cols.b, cols.c];
        for (let i = 0; i < 3; i++) {
          const ang = t * (0.06 + sp.flow * 0.22) * (i % 2 === 0 ? 1 : -1) + i * 2.1;
          const cx = w / 2 + Math.sin(ang * 1.3 + i) * w * 0.28;
          const cy = h / 2 + Math.cos(ang + i * 1.4) * h * 0.24;
          const len = Math.max(w, h) * (0.9 + i * 0.25);
          const g = ctx.createLinearGradient(
            cx - (Math.cos(ang) * len) / 2,
            cy - (Math.sin(ang) * len) / 2,
            cx + (Math.cos(ang) * len) / 2,
            cy + (Math.sin(ang) * len) / 2,
          );
          const wob = (Math.sin(t * 0.9 + i * 2.6) + 1) * 0.5;
          const alpha = (0.16 + f.settings.intensity * 0.2) * (0.7 + wob * 0.5) * (0.6 + sp.brightness * 0.7) * (0.85 + f.bass * 0.4);
          g.addColorStop(0, "rgba(0,0,0,0)");
          g.addColorStop(0.35 + wob * 0.2, rgba(palette[i], alpha));
          g.addColorStop(0.65 + wob * 0.1, rgba(palette[(i + 1) % 3], alpha * 0.6));
          g.addColorStop(1, "rgba(0,0,0,0)");
          ctx.fillStyle = g;
          ctx.fillRect(0, 0, w, h);
        }
        ctx.globalCompositeOperation = "source-over";
      },
    }),
    [],
  );

  return (
    <canvas
      key={mode}
      ref={canvasRef}
      className="side-viz-layer"
      aria-hidden="true"
      style={{ opacity }}
    />
  );
});
