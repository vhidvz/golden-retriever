// furtest.c — throwaway spike: ARGB GL overlay window on X11/XWayland with shell fur.
// Measures GPU time per frame for a dog-sized furred stand-in, and probes overlay
// behaviour (always-on-top, input shape, self-positioning, focus) on the live compositor.
// Build: gcc -O2 furtest.c -o furtest -lX11 -lXext -lXrender -lGL -lm
#define _GNU_SOURCE
#define GL_GLEXT_PROTOTYPES
#include <X11/Xlib.h>
#include <X11/Xutil.h>
#include <X11/Xatom.h>
#include <X11/extensions/shape.h>
#include <X11/extensions/Xrender.h>
#include <GL/gl.h>
#include <GL/glext.h>
#include <GL/glx.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef GLXContext (*CreateCtxAttribs)(Display *, GLXFBConfig, GLXContext, Bool, const int *);
typedef void (*SwapIntervalEXT)(Display *, GLXDrawable, int);

static double now_s(void) {
  struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
  return t.tv_sec + t.tv_nsec * 1e-9;
}
static void sleep_until(double t) {
  double d = t - now_s();
  if (d > 0) { struct timespec ts = {(time_t)d, (long)((d - (time_t)d) * 1e9)}; nanosleep(&ts, NULL); }
}

// ---------- minimal column-major matrix helpers ----------
static void m4_mul(float *o, const float *a, const float *b) {
  float r[16];
  for (int c = 0; c < 4; c++) for (int rr = 0; rr < 4; rr++) {
    r[c * 4 + rr] = 0;
    for (int k = 0; k < 4; k++) r[c * 4 + rr] += a[k * 4 + rr] * b[c * 4 + k];
  }
  memcpy(o, r, sizeof r);
}
static void m4_identity(float *m) { memset(m, 0, 64); m[0] = m[5] = m[10] = m[15] = 1; }
static void m4_ortho(float *m, float l, float r, float b, float t, float n, float f) {
  m4_identity(m);
  m[0] = 2 / (r - l); m[5] = 2 / (t - b); m[10] = -2 / (f - n);
  m[12] = -(r + l) / (r - l); m[13] = -(t + b) / (t - b); m[14] = -(f + n) / (f - n);
}

// ---------- shaders ----------
static const char *VS =
"#version 330 core\n"
"layout(location=0) in vec3 aPos; layout(location=1) in vec2 aUV;\n"
"uniform mat4 uModel; uniform mat3 uNormalM; uniform mat4 uVP;\n"
"uniform float uFurLen; uniform int uShells; uniform float uTime; uniform vec2 uUVScale;\n"
"out vec2 vUV; out vec3 vN; out float vShell;\n"
"void main(){\n"
"  float s = uShells>0 ? float(gl_InstanceID)/float(uShells) : 0.0;\n"
"  vec3 n = normalize(uNormalM*aPos);\n"
"  vec4 wp = uModel*vec4(aPos,1.0);\n"
"  vec3 droop = vec3(0.0,-1.0,0.0)*s*s*uFurLen*0.6 + vec3(sin(uTime*1.7+wp.y*6.0),0.0,0.0)*s*s*uFurLen*0.15;\n"
"  wp.xyz += n*uFurLen*s + droop;\n"
"  vUV = aUV*uUVScale; vN = n; vShell = s;\n"
"  gl_Position = uVP*wp;\n"
"}\n";
static const char *FS =
"#version 330 core\n"
"in vec2 vUV; in vec3 vN; in float vShell;\n"
"uniform int uA2C; uniform vec3 uLight;\n"
"out vec4 o;\n"
"float hash(vec2 p){ p = fract(p*vec2(123.34,456.21)); p += dot(p,p+45.32); return fract(p.x*p.y); }\n"
"void main(){\n"
"  vec2 cell = floor(vUV); vec2 f = fract(vUV)-0.5;\n"
"  float h = hash(cell); float len = 0.35+0.65*h; float r = length(f)*2.0; float cov = 1.0;\n"
"  if (vShell > 0.0) {\n"
"    float t = vShell/len; if (t > 1.0) discard;\n"
"    float edge = (1.0-t) - r;\n"
"    if (uA2C==1) { cov = clamp(edge/max(fwidth(r),1e-4)+0.5, 0.0, 1.0); if (cov <= 0.0) discard; }\n"
"    else if (edge < 0.0) discard;\n"
"  }\n"
"  float ndl = clamp(dot(normalize(vN), normalize(uLight))*0.5+0.5, 0.0, 1.0);\n"
"  float ao = mix(0.35, 1.0, vShell);\n"
"  vec3 base = mix(vec3(0.62,0.40,0.16), vec3(0.93,0.74,0.42), h);\n"
"  o = vec4(base*ao*(0.35+0.75*ndl), cov);\n"
"}\n";
static const char *VS2 =
"#version 330 core\n"
"layout(location=0) in vec3 aPos; uniform vec4 uRect; out vec2 vP;\n"
"void main(){ vP = aPos.xy; gl_Position = vec4(uRect.xy + aPos.xy*uRect.zw, 0.0, 1.0); }\n";
static const char *FS2 =
"#version 330 core\n"
"in vec2 vP; out vec4 o;\n"
"void main(){ float a = 0.45*smoothstep(1.0, 0.2, length(vP)); o = vec4(0.0,0.0,0.0,a); }\n";

static GLuint compile(GLenum type, const char *src) {
  GLuint s = glCreateShader(type); glShaderSource(s, 1, &src, NULL); glCompileShader(s);
  GLint ok; glGetShaderiv(s, GL_COMPILE_STATUS, &ok);
  if (!ok) { char log[2048]; glGetShaderInfoLog(s, sizeof log, NULL, log); fprintf(stderr, "shader: %s\n", log); exit(2); }
  return s;
}
static GLuint program(const char *vs, const char *fs) {
  GLuint p = glCreateProgram(); glAttachShader(p, compile(GL_VERTEX_SHADER, vs)); glAttachShader(p, compile(GL_FRAGMENT_SHADER, fs));
  glLinkProgram(p); GLint ok; glGetProgramiv(p, GL_LINK_STATUS, &ok);
  if (!ok) { char log[2048]; glGetProgramInfoLog(p, sizeof log, NULL, log); fprintf(stderr, "link: %s\n", log); exit(2); }
  return p;
}

// ---------- stand-in dog built from ellipsoids (height ~1 unit) ----------
typedef struct { float cx, cy, cz, rx, ry, rz; } Part;
static const Part PARTS[] = {
  { 0.00f, 0.55f, 0.00f, 0.42f, 0.20f, 0.17f}, // torso
  { 0.25f, 0.57f, 0.00f, 0.22f, 0.23f, 0.19f}, // chest
  { 0.45f, 0.72f, 0.00f, 0.13f, 0.18f, 0.12f}, // neck
  { 0.55f, 0.88f, 0.00f, 0.15f, 0.13f, 0.12f}, // head
  { 0.70f, 0.83f, 0.00f, 0.11f, 0.06f, 0.06f}, // muzzle
  { 0.50f, 0.88f, 0.11f, 0.05f, 0.11f, 0.03f}, // ear
  { 0.50f, 0.88f,-0.11f, 0.05f, 0.11f, 0.03f}, // ear
  { 0.30f, 0.20f, 0.10f, 0.055f,0.22f, 0.055f},// legs
  { 0.30f, 0.20f,-0.10f, 0.055f,0.22f, 0.055f},
  {-0.30f, 0.20f, 0.10f, 0.055f,0.22f, 0.055f},
  {-0.30f, 0.20f,-0.10f, 0.055f,0.22f, 0.055f},
  {-0.52f, 0.62f, 0.00f, 0.20f, 0.05f, 0.04f}, // tail
};
#define NPARTS (int)(sizeof PARTS / sizeof PARTS[0])

typedef struct { int shells, msaa, a2c; float ssaa; } Config;

int main(int argc, char **argv) {
  int W = 800, H = 720, dogpx = 600, fps = 50, orx = 0, move = 0, shape = 1, x0 = 300, y0 = 300, quiet_clicks = 0, opaque = 0, cull = 0, one_shells = 32, one_msaa = 4;
  double secs = 4.0; const char *type = "utility"; const char *sweep = "default"; const char *title = "buddy-spike";
  for (int i = 1; i < argc; i++) {
    if (!strcmp(argv[i], "--w")) W = atoi(argv[++i]); else if (!strcmp(argv[i], "--h")) H = atoi(argv[++i]);
    else if (!strcmp(argv[i], "--dogpx")) dogpx = atoi(argv[++i]); else if (!strcmp(argv[i], "--fps")) fps = atoi(argv[++i]);
    else if (!strcmp(argv[i], "--or")) orx = atoi(argv[++i]); else if (!strcmp(argv[i], "--move")) move = atoi(argv[++i]);
    else if (!strcmp(argv[i], "--shape")) shape = atoi(argv[++i]); else if (!strcmp(argv[i], "--secs")) secs = atof(argv[++i]);
    else if (!strcmp(argv[i], "--type")) type = argv[++i]; else if (!strcmp(argv[i], "--sweep")) sweep = argv[++i];
    else if (!strcmp(argv[i], "--x")) x0 = atoi(argv[++i]); else if (!strcmp(argv[i], "--y")) y0 = atoi(argv[++i]);
    else if (!strcmp(argv[i], "--title")) title = argv[++i]; else if (!strcmp(argv[i], "--cull")) cull = atoi(argv[++i]); else if (!strcmp(argv[i], "--opaque")) opaque = atoi(argv[++i]); else if (!strcmp(argv[i], "--shells")) one_shells = atoi(argv[++i]); else if (!strcmp(argv[i], "--msaa")) one_msaa = atoi(argv[++i]); else if (!strcmp(argv[i], "--quiet")) quiet_clicks = 1;
  }
  Display *dpy = XOpenDisplay(NULL);
  if (!dpy) { fprintf(stderr, "cannot open X display\n"); return 1; }
  int scr = DefaultScreen(dpy); Window root = RootWindow(dpy, scr);
  printf("INFO screen=%dx%d\n", DisplayWidth(dpy, scr), DisplayHeight(dpy, scr));

  int attrs[] = {GLX_X_RENDERABLE, True, GLX_DRAWABLE_TYPE, GLX_WINDOW_BIT, GLX_RENDER_TYPE, GLX_RGBA_BIT,
                 GLX_X_VISUAL_TYPE, GLX_TRUE_COLOR, GLX_RED_SIZE, 8, GLX_GREEN_SIZE, 8, GLX_BLUE_SIZE, 8,
                 GLX_ALPHA_SIZE, 8, GLX_DOUBLEBUFFER, True, None};
  int n = 0; GLXFBConfig *fbc = glXChooseFBConfig(dpy, scr, attrs, &n), chosen = NULL; XVisualInfo *vi = NULL;
  for (int i = 0; i < n && !chosen; i++) {
    XVisualInfo *v = glXGetVisualFromFBConfig(dpy, fbc[i]); if (!v) continue;
    XRenderPictFormat *pf = XRenderFindVisualFormat(dpy, v->visual);
    if (opaque ? (v->depth == 24) : (pf && pf->direct.alphaMask > 0 && v->depth == 32)) { chosen = fbc[i]; vi = v; } else XFree(v);
  }
  if (!chosen) { printf("RESULT argb_visual=none (fbconfigs=%d)\n", n); return 3; }
  printf("INFO argb_visual=0x%lx depth=%d\n", vi->visualid, vi->depth);

  XSetWindowAttributes swa = {0};
  swa.colormap = XCreateColormap(dpy, root, vi->visual, AllocNone);
  swa.border_pixel = 0; swa.background_pixel = 0; swa.override_redirect = orx ? True : False;
  swa.event_mask = ButtonPressMask | StructureNotifyMask | FocusChangeMask | VisibilityChangeMask | ExposureMask;
  Window win = XCreateWindow(dpy, root, x0, y0, W, H, 0, vi->depth, InputOutput, vi->visual,
                             CWColormap | CWBorderPixel | CWBackPixel | CWOverrideRedirect | CWEventMask, &swa);
  XStoreName(dpy, win, title);
  XClassHint ch = {(char *)"buddy-spike", (char *)"BuddySpike"}; XSetClassHint(dpy, win, &ch);
  XWMHints wmh = {0}; wmh.flags = InputHint; wmh.input = False; XSetWMHints(dpy, win, &wmh);
  XSizeHints sh = {0}; sh.flags = PPosition | PSize | USPosition; sh.x = x0; sh.y = y0; sh.width = W; sh.height = H;
  XSetWMNormalHints(dpy, win, &sh);
  long motif[5] = {2, 0, 0, 0, 0}; Atom mh = XInternAtom(dpy, "_MOTIF_WM_HINTS", False);
  XChangeProperty(dpy, win, mh, mh, 32, PropModeReplace, (unsigned char *)motif, 5);
  if (!orx) {
    Atom st[4] = {XInternAtom(dpy, "_NET_WM_STATE_ABOVE", False), XInternAtom(dpy, "_NET_WM_STATE_STICKY", False),
                  XInternAtom(dpy, "_NET_WM_STATE_SKIP_TASKBAR", False), XInternAtom(dpy, "_NET_WM_STATE_SKIP_PAGER", False)};
    XChangeProperty(dpy, win, XInternAtom(dpy, "_NET_WM_STATE", False), XA_ATOM, 32, PropModeReplace, (unsigned char *)st, 4);
    char tn[64]; snprintf(tn, sizeof tn, "_NET_WM_WINDOW_TYPE_%s", !strcmp(type, "dock") ? "DOCK" : !strcmp(type, "notification") ? "NOTIFICATION" : !strcmp(type, "normal") ? "NORMAL" : "UTILITY");
    Atom ty = XInternAtom(dpy, tn, False);
    XChangeProperty(dpy, win, XInternAtom(dpy, "_NET_WM_WINDOW_TYPE", False), XA_ATOM, 32, PropModeReplace, (unsigned char *)&ty, 1);
  }
  // Input shape: only an ellipse around the dog accepts pointer events; the rest is click-through.
  int sev, serr; int has_shape = XShapeQueryExtension(dpy, &sev, &serr);
  if (shape && has_shape) {
    XRectangle rects[48]; int nr = 0; float cx = W * 0.5f, cy = H * 0.5f, rx = dogpx * 0.48f, ry = dogpx * 0.42f;
    for (int i = 0; i < 48; i++) {
      float yy = -ry + (2 * ry) * (i + 0.5f) / 48, hw = rx * sqrtf(fmaxf(0, 1 - (yy * yy) / (ry * ry)));
      rects[nr].x = (short)(cx - hw); rects[nr].y = (short)(cy - ry + (2 * ry) * i / 48);
      rects[nr].width = (unsigned short)(2 * hw); rects[nr].height = (unsigned short)(2 * ry / 48 + 1); nr++;
    }
    XShapeCombineRectangles(dpy, win, ShapeInput, 0, 0, rects, nr, ShapeSet, Unsorted);
  }
  printf("INFO xshape=%d input_shape_applied=%d override_redirect=%d type=%s\n", has_shape, shape && has_shape, orx, orx ? "-" : type);

  CreateCtxAttribs cca = (CreateCtxAttribs)glXGetProcAddressARB((const GLubyte *)"glXCreateContextAttribsARB");
  int cattr[] = {GLX_CONTEXT_MAJOR_VERSION_ARB, 3, GLX_CONTEXT_MINOR_VERSION_ARB, 3, GLX_CONTEXT_PROFILE_MASK_ARB, GLX_CONTEXT_CORE_PROFILE_BIT_ARB, None};
  GLXContext ctx = cca ? cca(dpy, chosen, NULL, True, cattr) : NULL;
  if (!ctx) { printf("RESULT gl_context=failed\n"); return 4; }
  XMapWindow(dpy, win); XMoveWindow(dpy, win, x0, y0); XSync(dpy, False);
  glXMakeCurrent(dpy, win, ctx);
  SwapIntervalEXT si = (SwapIntervalEXT)glXGetProcAddressARB((const GLubyte *)"glXSwapIntervalEXT");
  if (si) si(dpy, win, 0);
  printf("INFO gl_renderer=\"%s\" gl_version=\"%s\"\n", glGetString(GL_RENDERER), glGetString(GL_VERSION));

  // unit sphere mesh
  enum { SEG = 48, RING = 32 };
  int nv = (SEG + 1) * (RING + 1), ni = SEG * RING * 6;
  float *vb = malloc(sizeof(float) * nv * 5); unsigned *ib = malloc(sizeof(unsigned) * ni); int k = 0;
  for (int r = 0; r <= RING; r++) for (int s = 0; s <= SEG; s++) {
    float v = (float)r / RING, u = (float)s / SEG, th = v * (float)M_PI, ph = u * 2 * (float)M_PI;
    vb[k++] = sinf(th) * cosf(ph); vb[k++] = cosf(th); vb[k++] = sinf(th) * sinf(ph); vb[k++] = u; vb[k++] = v;
  }
  k = 0;
  for (int r = 0; r < RING; r++) for (int s = 0; s < SEG; s++) {
    unsigned a = r * (SEG + 1) + s, b = a + SEG + 1;
    ib[k++] = a; ib[k++] = b; ib[k++] = a + 1; ib[k++] = a + 1; ib[k++] = b; ib[k++] = b + 1;
  }
  GLuint vao, vbo, ebo; glGenVertexArrays(1, &vao); glBindVertexArray(vao);
  glGenBuffers(1, &vbo); glBindBuffer(GL_ARRAY_BUFFER, vbo); glBufferData(GL_ARRAY_BUFFER, sizeof(float) * nv * 5, vb, GL_STATIC_DRAW);
  glGenBuffers(1, &ebo); glBindBuffer(GL_ELEMENT_ARRAY_BUFFER, ebo); glBufferData(GL_ELEMENT_ARRAY_BUFFER, sizeof(unsigned) * ni, ib, GL_STATIC_DRAW);
  glEnableVertexAttribArray(0); glVertexAttribPointer(0, 3, GL_FLOAT, GL_FALSE, 20, (void *)0);
  glEnableVertexAttribArray(1); glVertexAttribPointer(1, 2, GL_FLOAT, GL_FALSE, 20, (void *)12);
  GLuint prog = program(VS, FS), prog2 = program(VS2, FS2);
  // quad for the contact shadow (reuses attribute 0)
  GLuint vao2, vbo2; float quad[] = {-1, -1, 0, 1, -1, 0, -1, 1, 0, 1, 1, 0};
  glGenVertexArrays(1, &vao2); glBindVertexArray(vao2); glGenBuffers(1, &vbo2); glBindBuffer(GL_ARRAY_BUFFER, vbo2);
  glBufferData(GL_ARRAY_BUFFER, sizeof quad, quad, GL_STATIC_DRAW); glEnableVertexAttribArray(0); glVertexAttribPointer(0, 3, GL_FLOAT, GL_FALSE, 12, (void *)0);

  Config sweep_default[] = {{16, 4, 1, 1}, {24, 4, 1, 1}, {32, 4, 1, 1}, {48, 4, 1, 1}, {64, 4, 1, 1}, {32, 0, 0, 1}, {32, 0, 0, 1.5f}, {48, 0, 0, 1.5f}};
  Config sweep_one[] = {{32, 4, 1, 1}};
  Config *cfgs = sweep_default; int ncfg = 8;
  if (!strcmp(sweep, "one")) { sweep_one[0].shells = one_shells; sweep_one[0].msaa = one_msaa; sweep_one[0].a2c = one_msaa > 0; cfgs = sweep_one; ncfg = 1; }

  int clicks = 0, focus_in = 0, configures = 0; double t_start = now_s();
  GLuint q[2]; glGenQueries(2, q);
  for (int c = 0; c < ncfg; c++) {
    Config cf = cfgs[c]; int fw = (int)(W * cf.ssaa), fh = (int)(H * cf.ssaa);
    GLuint fbo, crb, drb; glGenFramebuffers(1, &fbo); glBindFramebuffer(GL_FRAMEBUFFER, fbo);
    glGenRenderbuffers(1, &crb); glBindRenderbuffer(GL_RENDERBUFFER, crb);
    if (cf.msaa) glRenderbufferStorageMultisample(GL_RENDERBUFFER, cf.msaa, GL_RGBA8, fw, fh); else glRenderbufferStorage(GL_RENDERBUFFER, GL_RGBA8, fw, fh);
    glGenRenderbuffers(1, &drb); glBindRenderbuffer(GL_RENDERBUFFER, drb);
    if (cf.msaa) glRenderbufferStorageMultisample(GL_RENDERBUFFER, cf.msaa, GL_DEPTH_COMPONENT24, fw, fh); else glRenderbufferStorage(GL_RENDERBUFFER, GL_DEPTH_COMPONENT24, fw, fh);
    glFramebufferRenderbuffer(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_RENDERBUFFER, crb);
    glFramebufferRenderbuffer(GL_FRAMEBUFFER, GL_DEPTH_ATTACHMENT, GL_RENDERBUFFER, drb);
    if (glCheckFramebufferStatus(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE) { printf("RESULT fbo=incomplete shells=%d\n", cf.shells); continue; }

    double t0 = now_s(), next = t0, gpu_sum = 0, gpu_max = 0, cpu_sum = 0, err_sum = 0; int frames = 0, gpu_n = 0, err_n = 0, have_prev = 0;
    int req_x = x0, req_y = y0;
    while (now_s() - t0 < secs) {
      while (XPending(dpy)) {
        XEvent e; XNextEvent(dpy, &e);
        if (e.type == ButtonPress) { clicks++; if (!quiet_clicks) { printf("EVENT click x=%d y=%d root=%d,%d t=%.2f\n", e.xbutton.x, e.xbutton.y, e.xbutton.x_root, e.xbutton.y_root, now_s() - t_start); fflush(stdout); } }
        else if (e.type == FocusIn) focus_in++;
        else if (e.type == ConfigureNotify) configures++;
      }
      double tf = now_s(); float tt = (float)(tf - t_start);
      if (move) {
        // check how far the window actually is from the previous request, then issue the next one
        int rx_, ry_; Window ch_; XTranslateCoordinates(dpy, win, root, 0, 0, &rx_, &ry_, &ch_);
        err_sum += fabs((double)(rx_ - req_x)) + fabs((double)(ry_ - req_y)); err_n++;
        req_x = x0 + (int)(500 * sinf(tt * 0.9f)); req_y = y0; XMoveWindow(dpy, win, req_x, req_y);
      }
      glBeginQuery(GL_TIME_ELAPSED, q[frames & 1]);
      glBindFramebuffer(GL_FRAMEBUFFER, fbo); glViewport(0, 0, fw, fh);
      glClearColor(0, 0, 0, 0); glClearDepth(1.0); glDepthMask(GL_TRUE); glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);
      // contact shadow (premultiplied alpha)
      glDisable(GL_DEPTH_TEST); glEnable(GL_BLEND); glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA);
      glUseProgram(prog2); glBindVertexArray(vao2);
      glUniform4f(glGetUniformLocation(prog2, "uRect"), 0.0f, -1.0f + 2.0f * (H * 0.5f - dogpx * 0.45f) / H, (dogpx * 0.5f) / (W * 0.5f), (dogpx * 0.07f) / (H * 0.5f));
      glDrawArrays(GL_TRIANGLE_STRIP, 0, 4);
      glDisable(GL_BLEND); glEnable(GL_DEPTH_TEST);
      if (cf.a2c && cf.msaa) { glEnable(GL_MULTISAMPLE); glEnable(GL_SAMPLE_ALPHA_TO_COVERAGE); glEnable(GL_SAMPLE_ALPHA_TO_ONE); }
      else { glDisable(GL_SAMPLE_ALPHA_TO_COVERAGE); glDisable(GL_SAMPLE_ALPHA_TO_ONE); }
      if (cull) { glEnable(GL_CULL_FACE); glCullFace(cull == 1 ? GL_BACK : GL_FRONT); } else glDisable(GL_CULL_FACE);
      glUseProgram(prog); glBindVertexArray(vao);
      float s = dogpx / 1.05f, vp[16];
      m4_ortho(vp, -W * 0.5f / s, W * 0.5f / s, -(H * 0.5f) / s + 0.48f, (H * 0.5f) / s + 0.48f, -3, 3);
      float ang = 0.5f * sinf(tt * 0.6f), ca = cosf(ang), sa = sinf(ang);
      glUniformMatrix4fv(glGetUniformLocation(prog, "uVP"), 1, GL_FALSE, vp);
      glUniform1f(glGetUniformLocation(prog, "uFurLen"), 0.035f); glUniform1i(glGetUniformLocation(prog, "uShells"), cf.shells);
      glUniform1f(glGetUniformLocation(prog, "uTime"), tt); glUniform2f(glGetUniformLocation(prog, "uUVScale"), 300.0f, 150.0f);
      glUniform1i(glGetUniformLocation(prog, "uA2C"), cf.a2c && cf.msaa); glUniform3f(glGetUniformLocation(prog, "uLight"), -0.4f, 0.8f, 0.5f);
      for (int p = 0; p < NPARTS; p++) {
        const Part *pt = &PARTS[p]; float bob = 0.01f * sinf(tt * 2.2f);
        // model = Ry * T * S ; normal matrix = Ry * S^-1
        float ry[16], tr[16], sc[16], m[16]; m4_identity(ry); ry[0] = ca; ry[2] = -sa; ry[8] = sa; ry[10] = ca;
        m4_identity(tr); tr[12] = pt->cx; tr[13] = pt->cy + bob; tr[14] = pt->cz;
        m4_identity(sc); sc[0] = pt->rx; sc[5] = pt->ry; sc[10] = pt->rz;
        m4_mul(m, tr, sc); m4_mul(m, ry, m);
        float nm[9] = {ca / pt->rx, 0, -sa / pt->rx, 0, 1 / pt->ry, 0, sa / pt->rz, 0, ca / pt->rz};
        glUniformMatrix4fv(glGetUniformLocation(prog, "uModel"), 1, GL_FALSE, m);
        glUniformMatrix3fv(glGetUniformLocation(prog, "uNormalM"), 1, GL_FALSE, nm);
        glDrawElementsInstanced(GL_TRIANGLES, ni, GL_UNSIGNED_INT, 0, cf.shells + 1);
      }
      glDisable(GL_CULL_FACE);
      glBindFramebuffer(GL_READ_FRAMEBUFFER, fbo); glBindFramebuffer(GL_DRAW_FRAMEBUFFER, 0);
      glBlitFramebuffer(0, 0, fw, fh, 0, 0, W, H, GL_COLOR_BUFFER_BIT, (fw == W) ? GL_NEAREST : GL_LINEAR);
      glEndQuery(GL_TIME_ELAPSED);
      if (frames == 10) {  // one-off readback: did the pipeline actually put pixels in the window's back buffer?
        unsigned char *px = malloc((size_t)W * H * 4); glBindFramebuffer(GL_READ_FRAMEBUFFER, 0); glReadBuffer(GL_BACK);
        glReadPixels(0, 0, W, H, GL_RGBA, GL_UNSIGNED_BYTE, px); long opaque = 0, partial = 0; int minx = W, maxx = 0, miny = H, maxy = 0;
        for (int y = 0; y < H; y++) for (int x = 0; x < W; x++) { unsigned char a = px[(y * W + x) * 4 + 3]; if (a == 255) opaque++; else if (a > 0) partial++; if (a > 200) { if (x < minx) minx = x; if (x > maxx) maxx = x; if (y < miny) miny = y; if (y > maxy) maxy = y; } }
        unsigned char *c = &px[((H / 2) * W + W / 2) * 4];
        printf("INFO readback opaque_px=%ld partial_px=%ld bbox=%dx%d centre_rgba=%d,%d,%d,%d\n", opaque, partial, maxx - minx + 1, maxy - miny + 1, c[0], c[1], c[2], c[3]); fflush(stdout); free(px);
      }
      glXSwapBuffers(dpy, win);
      if (frames == 30) {  // one-off: what does the X server hold for this window after presentation? (own window only)
        glFinish(); XSync(dpy, False);
        XImage *im = XGetImage(dpy, win, 0, 0, W, H, AllPlanes, ZPixmap);
        if (im) { long nz = 0, na = 0; for (int y = 0; y < H; y += 2) for (int x = 0; x < W; x += 2) { unsigned long p = XGetPixel(im, x, y); if (p & 0xffffff) nz++; if (p >> 24) na++; }
          printf("INFO xserver_window_image sampled_rgb_nonzero=%ld sampled_alpha_nonzero=%ld of=%d centre=0x%08lx\n", nz, na, (W / 2) * (H / 2), XGetPixel(im, W / 2, H / 2)); XDestroyImage(im); }
        else printf("INFO xserver_window_image=unavailable\n");
        fflush(stdout);
      }
      cpu_sum += now_s() - tf;
      if (have_prev) { GLuint64 ns = 0; glGetQueryObjectui64v(q[(frames + 1) & 1], GL_QUERY_RESULT, &ns); double ms = ns * 1e-6; gpu_sum += ms; if (ms > gpu_max) gpu_max = ms; gpu_n++; }
      have_prev = 1; frames++;
      next += 1.0 / fps; sleep_until(next);
    }
    double el = now_s() - t0;
    printf("RESULT shells=%d msaa=%d a2c=%d ssaa=%.1f fbo=%dx%d fps=%.1f gpu_ms_avg=%.2f gpu_ms_max=%.2f cpu_submit_ms_avg=%.2f", cf.shells, cf.msaa, cf.a2c, cf.ssaa, fw, fh, frames / el, gpu_n ? gpu_sum / gpu_n : 0, gpu_max, 1000 * cpu_sum / frames);
    if (move) printf(" move_err_px_avg=%.1f", err_n ? err_sum / err_n : -1);
    printf("\n"); fflush(stdout);
    glBindFramebuffer(GL_FRAMEBUFFER, 0); glDeleteFramebuffers(1, &fbo); glDeleteRenderbuffers(1, &crb); glDeleteRenderbuffers(1, &drb);
  }
  // final window state as the compositor left it
  XSync(dpy, False); struct timespec st = {0, 300000000}; nanosleep(&st, NULL);
  int rx_, ry_; Window ch_; XTranslateCoordinates(dpy, win, root, 0, 0, &rx_, &ry_, &ch_);
  Atom at; int fmt; unsigned long ni_, after; unsigned char *data = NULL; char states[512] = "";
  if (XGetWindowProperty(dpy, win, XInternAtom(dpy, "_NET_WM_STATE", False), 0, 32, False, XA_ATOM, &at, &fmt, &ni_, &after, &data) == Success && data) {
    for (unsigned long i = 0; i < ni_; i++) { char *nm = XGetAtomName(dpy, ((Atom *)data)[i]); strncat(states, nm + 14, sizeof states - strlen(states) - 2); strcat(states, ","); XFree(nm); }
    XFree(data);
  }
  printf("RESULT final_pos=%d,%d clicks_received=%d focus_in_events=%d configure_events=%d net_wm_state=[%s]\n", rx_, ry_, clicks, focus_in, configures, states);
  glXMakeCurrent(dpy, None, NULL); glXDestroyContext(dpy, ctx); XDestroyWindow(dpy, win); XCloseDisplay(dpy);
  return 0;
}
