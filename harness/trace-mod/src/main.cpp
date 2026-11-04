// Cadence trace mod: runs one official level with scripted inputs and a fixed frame time,
// recording player 1's state after every physics step.
//
// Driven by the file named in $CADENCE_TRACE_SPEC:
//   level 1            main level id
//   steps 2400         stop after this many physics steps
//   dt 0.016666668     frame time forced on every update (seconds)
//   out /path/t.csv    where to write the trace
//   press 120 1        at step 120, press button 1 (jump) for player 1
//   release 150 1
// The game exits after writing the trace (without saving).

#include <Geode/Geode.hpp>
#include <Geode/modify/GJBaseGameLayer.hpp>
#include <Geode/modify/PlayerObject.hpp>
#include <Geode/modify/MenuLayer.hpp>
#include <Geode/modify/GJSearchObject.hpp>
#include <Geode/modify/GameLevelManager.hpp>
#include <Geode/modify/GJAccountManager.hpp>
#include <Geode/modify/PlayLayer.hpp>
#include <Geode/modify/EffectGameObject.hpp>
#include <Geode/modify/CCEGLView.hpp>
#include <Geode/modify/CCScheduler.hpp>
#include <objc/message.h>
#include <objc/runtime.h>
#include <CoreFoundation/CoreFoundation.h>
#include <CoreGraphics/CGGeometry.h>

#include <chrono>
#include <cstdio>
#include <functional>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <map>
#include <set>
#include <typeinfo>
#include <tuple>
#include <sstream>
#include <string>
#include <unistd.h>
#include <libproc.h>
#include <mach/mach_time.h>
#include <sys/resource.h>
#include <vector>

using namespace geode::prelude;

namespace {

struct Input {
    bool down;
    int button;
};

struct Spec {
    int level = 1;
    int steps = 2400;
    float dt = 1.0f / 60.0f;
    std::string out = "/tmp/cadence-trace.csv";  // "%d" is replaced by the attempt number
    std::vector<std::multimap<int, Input>> attempts{1};
    int speed = 1;  // game updates per rendered frame (fast-forward)
    std::string dumpObjects;  // if set: dump the object table here and exit instead of playing
    std::string dumpLevel;    // if set: dump the loaded level's objects at the first step
    std::string levelString;  // if set: play this raw level string file instead of a main level
    bool noclip = false;      // noclip: the player can't die (the start's anti-cheat check aside)
    bool corner = false;      // corner: the window on top in the main screen's bottom-right corner,
                              // never taking focus (benches while the machine is in use)
    bool mute = false;        // mute: music and sound effects at volume 0 (still played and mixed)
    float benchSecs = 0;      // bench <seconds> <file>: real frame times (no forced dt, no trace),
    std::string benchOut;     // the player can't die, frame intervals written to the file
    std::string dumpSprites;  // if set: dump every object's sprite tree here and exit
    std::string dumpOffsets;  // if set: write field offsets (from offsets.inc) here and exit
    std::vector<std::pair<int, std::string>> screenshots;  // (step, path): save the frame there
    std::vector<std::pair<std::string, std::string>> guis;  // (menu name, path): menu screenshots
    std::string dumpAchievements;  // every achievement's fields, tab-separated, in the game's order
    int pauseStep = -1;            // pausegui <step> <path>: pause there, capture the pause menu
    std::string pausePath;
    std::string endPath;           // endgui <path>: let the level complete, capture the end screen
    std::vector<std::pair<int, std::string>> particles;    // (step, path): dump live particle systems
    std::vector<std::pair<int, std::string>> drawNodes;    // (step, path): dump CCDrawNode triangles (streaks)
    std::string triggers;                                  // log every trigger activation here
    int watchObj = -1;                                     // debugrect: also log this object's rect (unique id)
    int debugRectFrom = -1, debugRectTo = -1;              // log the player's cached rect around
    std::string debugRect;                                 // collisions and mode switches here
    std::vector<std::pair<int, std::string>> watchGroups;  // (group, path): positions every step
    int watchMax = 4;                                      // objects per watched group (watchmax N)
};

Spec g_spec;
bool g_active = false;
bool g_launched = false;
size_t g_attempt = 0;
PlayLayer* g_layer = nullptr;  // the PlayLayer of the current attempt; null while switching
int g_step = 0;
int g_loopIndex = 0;  // which of the frame's `speed` updates is running
int g_updates = 0;
FILE* g_out = nullptr;
FILE* g_out_main();
FILE* g_out2 = nullptr;  // player 2 while dual mode is on (<attempt file>.p2)

bool loadSpec() {
    char const* path = std::getenv("CADENCE_TRACE_SPEC");
    if (!path) return false;
    std::ifstream file(path);
    if (!file) return false;
    std::string line;
    while (std::getline(file, line)) {
        std::istringstream in(line);
        std::string cmd;
        in >> cmd;
        if (cmd == "level") in >> g_spec.level;
        else if (cmd == "steps") in >> g_spec.steps;
        else if (cmd == "dt") in >> g_spec.dt;
        else if (cmd == "out") in >> g_spec.out;
        else if (cmd == "dumpobjects") in >> g_spec.dumpObjects;
        else if (cmd == "dumplevel") in >> g_spec.dumpLevel;
        else if (cmd == "debugrect") in >> g_spec.debugRectFrom >> g_spec.debugRectTo >> g_spec.debugRect;
        else if (cmd == "watchobj") in >> g_spec.watchObj;
        else if (cmd == "levelstring") in >> g_spec.levelString;
        else if (cmd == "bench") in >> g_spec.benchSecs >> g_spec.benchOut;
        else if (cmd == "noclip") g_spec.noclip = true;
        else if (cmd == "corner") g_spec.corner = true;
        else if (cmd == "mute") g_spec.mute = true;
        else if (cmd == "dumpsprites") in >> g_spec.dumpSprites;
        else if (cmd == "dumpoffsets") in >> g_spec.dumpOffsets;
        else if (cmd == "speed") in >> g_spec.speed;
        else if (cmd == "screenshot") {
            int step = 0;
            std::string path;
            in >> step >> path;
            g_spec.screenshots.emplace_back(step, path);
        }
        else if (cmd == "triggers") in >> g_spec.triggers;
        else if (cmd == "dumpachievements") in >> g_spec.dumpAchievements;
        else if (cmd == "pausegui") in >> g_spec.pauseStep >> g_spec.pausePath;
        else if (cmd == "endgui") in >> g_spec.endPath;
        else if (cmd == "gui") {
            std::string name, path;
            in >> name >> path;
            g_spec.guis.emplace_back(name, path);
        }
        else if (cmd == "watchmax") in >> g_spec.watchMax;
        else if (cmd == "watchgroup") {
            int group = 0;
            std::string path;
            in >> group >> path;
            g_spec.watchGroups.emplace_back(group, path);
        }
        else if (cmd == "drawnodes") {
            int step = 0;
            std::string path;
            in >> step >> path;
            g_spec.drawNodes.emplace_back(step, path);
        }
        else if (cmd == "particles") {
            int step = 0;
            std::string path;
            in >> step >> path;
            g_spec.particles.emplace_back(step, path);
        }
        else if (cmd == "attempt") {
            if (!g_spec.attempts.back().empty()) g_spec.attempts.emplace_back();
        }
        else if (cmd == "press" || cmd == "release") {
            int step = 0, button = 1;
            in >> step >> button;
            g_spec.attempts.back().emplace(step, Input{cmd == "press", button});
        }
    }
    return true;
}

std::string attemptPath() {
    std::string path = g_spec.out;
    if (auto at = path.find("%d"); at != std::string::npos) path.replace(at, 2, std::to_string(g_attempt));
    return path;
}

GJGameLevel* loadLevel();

// Start the current attempt in a fresh PlayLayer (no state or pending timers carried over).
void startAttempt() {
    g_step = 0;
    g_updates = 0;
    auto scene = PlayLayer::scene(loadLevel(), false);
    CCDirector::get()->replaceScene(scene);
    // PlayLayer::get() still returns the old layer until the scene switch happens next frame.
    g_layer = scene->getChildByType<PlayLayer>(0);
}

FILE* g_out_main() { return g_out; }

void finish(char const* reason) {
    if (g_out) {
        std::fprintf(g_out, "# end %s step=%d updates=%d\n", reason, g_step, g_updates);
        std::fclose(g_out);
        g_out = nullptr;
    }
    if (g_out2) {
        std::fprintf(g_out2, "# end %s step=%d updates=%d\n", reason, g_step, g_updates);
        std::fclose(g_out2);
        g_out2 = nullptr;
    }
    g_layer = nullptr;
    if (++g_attempt < g_spec.attempts.size()) {
        queueInMainThread([] { startAttempt(); });
        return;
    }
    // Leave without running the game's exit path, so nothing is saved.
    _exit(0);
}

unsigned flags(PlayerObject* p) {
    bool const bits[] = {
        p->m_isOnGround, p->m_isUpsideDown, p->m_isDead, p->m_isShip, p->m_isBall,
        p->m_isBird, p->m_isDart, p->m_isRobot, p->m_isSpider, p->m_isSwing,
        p->m_isSideways, p->m_isGoingLeft, p->m_isPlatformer, p->m_isDashing,
        p->m_isOnSlope, p->m_isAccelerating,
    };
    unsigned out = 0;
    for (unsigned i = 0; i < sizeof bits; i++) out |= unsigned(bits[i]) << i;
    return out;
}

// One row per physics step, sampled at the end of the step (after collisions).
// x/y are the node position: the per-step physics position. fx/fy (m_position) is only
// synced once per rendered frame.
void record(PlayerObject* p, float dt, FILE* out = nullptr) {
    FILE* g_out = out ? out : ::g_out_main();
    auto pos = p->getPosition();
    auto layer = PlayLayer::get();
    auto cam = layer ? layer->m_gameState.m_cameraPosition : CCPoint{0, 0};
    auto outer = p->getObjectRect();
    auto inner = p->getObjectRect(0.3f, 0.3f);
    // Camera and ground state (raw GJBaseGameLayer fields, m1 2.2091): zoom +0x1a8, camera
    // offset y +0x1b4, +0x23c, dual-static flag +0x2a0, corridor height +0x2ec, corridor center
    // +0x308, ground tween progress +0x3f4, camera x/y +0x4b4/+0x4b8, +0x3154, ground nodes' y,
    // getMinPortalY (0xe1768) / getMaxPortalY (0xe1844).
    float zoom = 0, camOffY = 0, f23c = 0, modeH = 0, center = 0, groundT = 0, camX = 0, camY = 0, f3154 = 0;
    float g1 = 0, g2 = 0, minY = 0, maxY = 0;
    int dualStatic = 0;
    if (auto gl = static_cast<GJBaseGameLayer*>(layer)) {
        auto raw = reinterpret_cast<char*>(gl);
        auto rf = [&](size_t off) { float v; std::memcpy(&v, raw + off, sizeof v); return v; };
        zoom = rf(0x1a8); camOffY = rf(0x1b4); f23c = rf(0x23c); modeH = rf(0x2ec); center = rf(0x308);
        groundT = rf(0x3f4); camX = rf(0x4b4); camY = rf(0x4b8); f3154 = rf(0x3154);
        dualStatic = *reinterpret_cast<unsigned char*>(raw + 0x2a0);
        auto ground = [&](size_t off) {
            auto node = *reinterpret_cast<CCNode**>(raw + off);
            return node ? node->getPosition().y : 0.f;
        };
        g1 = ground(0xf78); g2 = ground(0xf80);
        using Getter = float (*)(GJBaseGameLayer*);
        minY = reinterpret_cast<Getter>(geode::base::get() + 0xe1768)(gl);
        maxY = reinterpret_cast<Getter>(geode::base::get() + 0xe1844)(gl);
    }
    std::fprintf(g_out, "%d,%.9g,%.9g,%.9g,%.17g,%.9g,%u,%.9g,%.9g,%.17g,%.9g,%.9g,%.9g,%.9g,%d,%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%d,%d,%.9g,%.17g,%d,%.9g,%d,%d,"
        "%.9g,%.9g,%.9g,%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g\n",
        g_step, dt, pos.x, pos.y, p->m_yVelocity, p->getRotation(), flags(p),
        p->m_playerSpeed, p->m_vehicleSize, p->m_gravity, p->m_position.x, p->m_position.y, cam.x, cam.y,
        int(p->m_touchingGround), int(p->m_isRotating), p->m_rotationSpeed,
        outer.size.width, outer.size.height, inner.size.width, inner.size.height, p->getScaleX(),
        int(p->m_isOnSlope), int(p->m_wasOnSlope), p->m_slopeVelocity, p->m_currentSlopeYVelocity,
        int(p->m_isCollidingWithSlope), p->m_slopeAngle, int(p->m_isCurrentSlopeTop), int(p->m_isRising),
        zoom, camOffY, f23c, dualStatic, modeH, center, groundT, camX, camY, f3154, g1, g2, minY, maxY);
}

float g_stepDt = 0;

// Debug: raw field dump at fixed offsets (for reverse engineering), printed once per step 0..3.
template <class T> T rawField(void* obj, size_t off) {
    T value;
    std::memcpy(&value, static_cast<char*>(obj) + off, sizeof value);
    return value;
}

void dumpRaw(PlayerObject* p) {
    std::fprintf(g_out, "# raw step=%d d788=%.17g f9ac=%.9g f5f0=%.9g da98=%.17g dac0=%.17g d560=%.17g da40=%.17g dae0=%.17g f9d0=%.9g f698=%.9g\n",
        g_step, rawField<double>(p, 0x788), rawField<float>(p, 0x9ac), rawField<float>(p, 0x5f0),
        rawField<double>(p, 0xa98), rawField<double>(p, 0xac0), rawField<double>(p, 0x560),
        rawField<double>(p, 0xa40), rawField<double>(p, 0xae0), rawField<float>(p, 0x9d0),
        rawField<float>(p, 0x698));
}

// Create every object id at the origin and record what the game says about it.
void dumpObjects(std::string const& path) {
    FILE* out = std::fopen(path.c_str(), "w");
    std::fprintf(out, "id,type,width,height,rect_x,rect_y,rect_w,rect_h,radius,no_touch,passable,decoration,trigger,solid_color\n");
    for (int id = 1; id <= 5000; id++) {
        auto obj = GameObject::createWithKey(id);
        if (!obj) continue;
        obj->setPosition({0, 0});
        auto rect = obj->getObjectRect();
        std::fprintf(out, "%d,%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%d,%d,%d,%d,%d\n", id,
            int(obj->m_objectType), obj->m_width, obj->m_height, rect.origin.x, rect.origin.y,
            rect.size.width, rect.size.height, obj->m_objectRadius, obj->m_isNoTouch,
            obj->m_isPassable, obj->m_isDecoration, obj->m_isTrigger, obj->m_isSolidColorBlock);
    }
    std::fclose(out);
}

// Every object in the running level, as the game sees it after loading.
void dumpLevelObjects(CCArray* objects, std::string const& path) {
    FILE* out = std::fopen(path.c_str(), "w");
    std::fprintf(out, "index,id,type,x,y,rect_x,rect_y,rect_w,rect_h,width,height,radius,rot,scale_x,scale_y,no_touch,passable,decoration,trigger,flip_x,flip_y,slope_dir,slope_uphill,slope_hazard,unique_id\n");
    int index = 0;
    for (auto obj : CCArrayExt<GameObject*>(objects)) {
        auto pos = obj->getPosition();
        auto rect = obj->getObjectRect();
        std::fprintf(out, "%d,%d,%d,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%.9g,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d\n",
            index++, obj->m_objectID, int(obj->m_objectType), pos.x, pos.y, rect.origin.x,
            rect.origin.y, rect.size.width, rect.size.height, obj->m_width, obj->m_height,
            obj->m_objectRadius, obj->getRotation(), obj->getScaleX(), obj->getScaleY(),
            obj->m_isNoTouch, obj->m_isPassable, obj->m_isDecoration, obj->m_isTrigger,
            int(obj->isFlipX()), int(obj->isFlipY()), int(obj->m_slopeDirection),
            int(obj->m_slopeUphill), int(obj->m_slopeIsHazard), obj->m_uniqueID);
    }
    std::fclose(out);
}

// Reverse lookup from (texture, rect) to sprite frame name, built from the frame cache.
std::map<std::tuple<void*, int, int, int, int>, std::string> frameNames() {
    std::map<std::tuple<void*, int, int, int, int>, std::string> names;
    for (auto [name, frame] : CCDictionaryExt<std::string, CCSpriteFrame*>(CCSpriteFrameCache::get()->m_pSpriteFrames)) {
        auto r = frame->getRect();
        names[{frame->getTexture(), int(r.origin.x * 4), int(r.origin.y * 4), int(r.size.width * 4), int(r.size.height * 4)}] = name;
    }
    return names;
}

void dumpNode(FILE* out, CCNode* node, std::string const& path, GameObject* obj,
              std::map<std::tuple<void*, int, int, int, int>, std::string> const& names) {
    auto sprite = typeinfo_cast<CCSprite*>(node);
    std::string frame = "-";
    if (sprite && sprite->getTexture()) {
        auto r = sprite->getTextureRect();
        auto it = names.find({sprite->getTexture(), int(r.origin.x * 4), int(r.origin.y * 4), int(r.size.width * 4), int(r.size.height * 4)});
        frame = it != names.end() ? it->second : "?";
    }
    char const* role = node == obj ? "main" : node == obj->m_colorSprite ? "detail" : node == obj->m_glowSprite ? "glow" : "child";
    auto pos = node->getPosition();
    auto anchor = node->getAnchorPoint();
    auto size = node->getContentSize();
    std::fprintf(out, "%d\t%s\t%s\t%s\t%.9g\t%.9g\t%.9g\t%.9g\t%.9g\t%.9g\t%.9g\t%.9g\t%.9g\t%d\t%d\t%d\t%d\t%d\n",
        obj->m_objectID, path.c_str(), role, frame.c_str(), pos.x, pos.y, anchor.x, anchor.y,
        size.width, size.height, node->getScaleX(), node->getScaleY(), node->getRotation(),
        node->getZOrder(), node->isVisible(), sprite ? sprite->isFlipX() : 0, sprite ? sprite->isFlipY() : 0,
        sprite ? int(sprite->getBlendFunc().dst) : 0);
    int i = 0;
    for (auto child : CCArrayExt<CCNode*>(node->getChildren())) {
        dumpNode(out, child, path + "/" + std::to_string(i++), obj, names);
    }
}

void dumpSpritesOf(std::vector<GameObject*> const& objects, std::string const& path) {
    auto names = frameNames();
    FILE* out = std::fopen(path.c_str(), "w");
    std::fprintf(out, "id\tpath\trole\tframe\tx\ty\tanchor_x\tanchor_y\tw\th\tscale_x\tscale_y\trot\tz\tvisible\tflip_x\tflip_y\tblend_dst\n");
    FILE* meta = std::fopen((path + ".objects").c_str(), "w");
    std::fprintf(meta, "id\tframe\tbase_color\tdetail_color\tz_layer\tz_order\tblending\thas_color_sprite\tglow\tcolor_z_related\tenhanced\trotation_speed\tdisable_rotation\tanimation_speed\tuses_audio_scale\tno_audio_scale\tcustom_audio_scale\tmin_audio_scale\tmax_audio_scale\trotation_base\trotation_rate\tobject_black\tcolor_sprite_black\tblack_child_opacity\tinvisible\thide\tcustom_glow_color\tglow_lbg\n");
    std::set<int> seen;
    for (auto obj : objects) {
        int id = obj->m_objectID;
        if (!seen.insert(id).second) continue;
        char const* frame = ObjectToolbox::sharedState()->intKeyToFrame(id);
        auto field = [&](int off) { return reinterpret_cast<char*>(obj) + off; };
        auto enhanced = typeinfo_cast<EnhancedGameObject*>(obj);
        std::fprintf(meta, "%d\t%s\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%d\t%.9g\t%d\t%.9g\t%d\t%d\t%d\t%.9g\t%.9g\t%.9g\t%.9g\t%d\t%d\t%.9g\t%d\t%d\t%d\t%d\n", id, frame ? frame : "-",
            obj->m_baseColor ? obj->m_baseColor->m_defaultColorID : -1,
            obj->m_detailColor ? obj->m_detailColor->m_defaultColorID : -1,
            int(obj->m_defaultZLayer), obj->m_defaultZOrder, obj->m_baseOrDetailBlending,
            obj->m_hasColorSprite, obj->m_glowSprite != nullptr,
            int(*reinterpret_cast<unsigned char*>(reinterpret_cast<char*>(obj) + 0x41d)),  // m_colorZLayerRelated
            enhanced != nullptr,
            enhanced ? *reinterpret_cast<float*>(field(0x570)) : 0.0f,  // m_rotationSpeed
            enhanced ? int(*reinterpret_cast<unsigned char*>(field(0x56c))) : 0,  // m_disableRotation
            enhanced ? *reinterpret_cast<float*>(field(0x57c)) : 0.0f,  // m_rotationAnimationSpeed
            int(*reinterpret_cast<unsigned char*>(field(0x3c8))),  // m_usesAudioScale
            int(*reinterpret_cast<unsigned char*>(field(0x3c9))),  // m_hasNoAudioScale
            int(*reinterpret_cast<unsigned char*>(field(0x41e))),  // m_customAudioScale
            *reinterpret_cast<float*>(field(0x420)), *reinterpret_cast<float*>(field(0x424)),  // m_min/maxAudioScale
            enhanced ? *reinterpret_cast<float*>(field(0x574)) : 0.0f,  // m_rotationAngle (spin magnitude)
            enhanced ? *reinterpret_cast<float*>(field(0x578)) : 0.0f,  // m_rotationDelta (spin rate, degrees/s)
            int(*reinterpret_cast<unsigned char*>(field(0x281))),  // m_isObjectBlack
            int(*reinterpret_cast<unsigned char*>(field(0x280))),  // m_isColorSpriteBlack
            *reinterpret_cast<float*>(field(0x284)),  // m_blackChildOpacity
            int(*reinterpret_cast<unsigned char*>(field(0x3e7))),  // m_isInvisible
            int(*reinterpret_cast<unsigned char*>(field(0x50e))),  // m_isHide
            int(*reinterpret_cast<unsigned char*>(field(0x431))),  // m_customGlowColor
            int(*reinterpret_cast<unsigned char*>(field(0x432))));  // m_glowColorIsLBG
        dumpNode(out, obj, "0", obj, names);
        // the glow (and, when it isn't a child, the color sprite) live in other batch nodes
        if (obj->m_glowSprite) dumpNode(out, obj->m_glowSprite, "glow", obj, names);
        if (obj->m_colorSprite && obj->m_colorSprite->getParent() != obj) dumpNode(out, obj->m_colorSprite, "color", obj, names);
    }
    std::fclose(meta);
    std::fclose(out);
}

void dumpSprites(std::string const& path) {
    std::vector<GameObject*> objects;
    for (int id = 1; id <= 5000; id++) {
        if (auto obj = GameObject::createWithKey(id)) objects.push_back(obj);
    }
    dumpSpritesOf(objects, path);
}

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Winvalid-offsetof"
void dumpOffsets(std::string const& path) {
    FILE* out = std::fopen(path.c_str(), "w");
#define OFF(cls, field) std::fprintf(out, "%s %s 0x%zx\n", #cls, #field, offsetof(cls, field));
#include "offsets.inc"
#undef OFF
    std::fclose(out);
}
#pragma clang diagnostic pop

GJGameLevel* loadLevel() {
    if (!g_spec.levelString.empty()) {
        std::ifstream file(g_spec.levelString);
        std::stringstream text;
        text << file.rdbuf();
        auto level = GJGameLevel::create();
        level->m_levelString = text.str();
        level->m_levelName = "cadence-synthetic";
        return level;
    }
    return GameLevelManager::get()->getMainLevel(g_spec.level, false);
}

// True when `layer` is the PlayLayer of the running attempt. The previous attempt's layer keeps
// updating until the scene switch on the next frame, so checking PlayLayer::get() is not enough.
bool recording(GJBaseGameLayer* layer) {
    return g_active && g_layer && layer == g_layer && g_spec.benchSecs <= 0;
}

} // namespace

void dumpParticles(GJBaseGameLayer* layer, std::string const& path, int step);
void dumpDrawNodes(GJBaseGameLayer* layer, std::string const& path, int step);
void watchGroup(GJBaseGameLayer* layer, int group, std::string const& path, int step);
int currentStep();
FILE* triggerLog();

// AppKit through the Objective-C runtime (this file is C++).
template <class R = id, class... A>
R msg(id self, char const* sel, A... args) {
    return reinterpret_cast<R (*)(id, SEL, A...)>(objc_msgSend)(self, sel_registerName(sel), args...);
}
void noActivate(id, SEL, BOOL) {}
void noActivateNow(id, SEL) {}

// `corner`: the app never activates itself (both of NSApplication's activate calls do nothing)...
void keepInBackground() {
    Class app = objc_getClass("NSApplication");
    if (Method m = class_getInstanceMethod(app, sel_registerName("activateIgnoringOtherApps:"))) method_setImplementation(m, (IMP)noActivate);
    if (Method m = class_getInstanceMethod(app, sel_registerName("activate"))) method_setImplementation(m, (IMP)noActivateNow);
}

// ...and its window floats on top, parked past the main screen's bottom-right corner: only its
// top-left 320 x 200 points show, just above the Dock, and the mouse passes through. It stays
// visible, so frames keep the display's pace, and the drawn size is unchanged (the window is
// 1280 x 720 points: GD draws 1280 x 720 pixels and the system scales them up). Cadence's
// CADENCE_CORNER parks its window the same way.
// The game stops drawing while its window is covered, so this runs from a run loop timer (from
// launch, before any frame) rather than a frame hook, and again whenever the window resizes.
void moveToCorner(CFRunLoopTimerRef, void*) {
    id app = msg((id)objc_getClass("NSApplication"), "sharedApplication");
    id screen = msg(msg((id)objc_getClass("NSScreen"), "screens"), "firstObject");
    if (!screen) return;
    CGRect sf = msg<CGRect>(screen, "visibleFrame");  // (y up; stops at the Dock and the menu bar)
    id windows = msg(app, "windows");
    for (unsigned long i = 0, n = msg<unsigned long>(windows, "count"); i < n; i++) {
        id w = msg(windows, "objectAtIndex:", i);
        if (!msg<BOOL>(w, "isVisible")) continue;
        CGRect f = msg<CGRect>(w, "frame");
        CGPoint to{sf.origin.x + sf.size.width - 320, sf.origin.y + 200 - f.size.height};
        if (f.origin.x == to.x && f.origin.y == to.y && msg<long>(w, "level") == 3) continue;
        log::info("cadence-trace: corner: window {}x{} at {},{} -> {},{} (app active {})", f.size.width, f.size.height,
                  f.origin.x, f.origin.y, to.x, to.y, (int)msg<BOOL>(app, "isActive"));
        msg<void>(w, "setLevel:", 3L);  // NSFloatingWindowLevel
        msg<void>(w, "setFrameOrigin:", to);
        msg<void>(w, "setIgnoresMouseEvents:", (BOOL)YES);
        msg<void>(w, "orderFrontRegardless");
    }
}

$on_mod(Loaded) {
    g_active = loadSpec();
    if (g_active && g_spec.corner) {
        keepInBackground();
        auto timer = CFRunLoopTimerCreate(nullptr, CFAbsoluteTimeGetCurrent() + 0.1, 0.25, 0, 0, moveToCorner, nullptr);
        CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
    }
    if (g_active) log::info("cadence-trace: level {} steps {} -> {}", g_spec.level, g_spec.steps, g_spec.out);
}

// A whole frame needs a window (full screen on a notched display hides the top rows): the
// harness's own save sets windowed (game variable 0025) at resolution key 8, 1280 x 720
// (`GameManager::resolutionForKey` 0x303498), which GD applies at launch.

// `dumpachievements <path>`: AchievementManager's m_allAchievements (CCDictionaries of strings),
// one per line: identifier, name, achievedDescription, unachievedDescription, icon, limits.
void dumpAchievements(std::string const& path) {
    FILE* out = std::fopen(path.c_str(), "w");
    if (!out) return;
    auto field = [](CCDictionary* d, char const* key) -> std::string {
        auto v = typeinfo_cast<CCString*>(d->objectForKey(key));
        std::string s = v ? v->getCString() : "";
        for (auto& c : s) if (c == '\t' || c == '\n') c = ' ';
        return s;
    };
    auto all = AchievementManager::sharedState()->m_allAchievements;
    if (all->count()) {
        std::fprintf(out, "#keys");
        for (auto [k, v] : CCDictionaryExt<std::string, CCObject*>(static_cast<CCDictionary*>(all->objectAtIndex(0)))) std::fprintf(out, " %s", k.c_str());
        std::fprintf(out, "\n");
    }
    for (auto d : CCArrayExt<CCDictionary*>(all)) {
        std::fprintf(out, "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n", field(d, "identifier").c_str(), field(d, "title").c_str(),
            field(d, "achievedDescription").c_str(), field(d, "unachievedDescription").c_str(), field(d, "icon").c_str(),
            field(d, "limits").c_str(), field(d, "order").c_str(), field(d, "un").c_str());
    }
    std::fclose(out);
}

// Menu screenshots (`gui <name> <path>`): from a fresh main menu, open the menu, let its
// transition finish, save the frame; then the next one; then quit.
size_t g_guiIndex = 0;
int g_guiFrame = -1;
int g_guiTab = -1;  // garageN: the icon kit's tab to select once it's up
int g_guiItem = -1;  // <layer>:N: the Nth menu button (depth first) to press once it's up
std::vector<int> g_guiMore;  // creator:N/M/K: more buttons, pressed 150 frames apart
int g_guiShotFrame = 140;  // when the frame is saved (online screens wait for the server)
int g_pauseFrames = -1;  // frames since pausegui paused the level
int g_endFrames = -1;    // frames since endgui let the level complete

void openGui(std::string const& name) {
    auto scene = CCDirector::get()->getRunningScene();
    if (name == "settings") {
        auto l = OptionsLayer::create();
        scene->addChild(l, 100);
        l->showLayer(true);
    } else if (name == "options") {
        MoreOptionsLayer::create()->show();
    } else if (name.rfind("gs", 0) == 0) {
        // gs<tab>[p<page>]: GameSettingsLayer (what Settings > Options opens in 2.2091)
        int tab = std::atoi(name.c_str() + 2), page = 0;
        if (auto p = name.find('p'); p != std::string::npos) page = std::atoi(name.c_str() + p + 1);
        auto l = GameSettingsLayer::create(nullptr, 0);
        l->show();
        l->showTab(tab);
        for (int i = 0; i < page; i++) if (l->m_nextPageButton) l->m_nextPageButton->activate();
    } else if (name == "keys") {
        auto l = GameSettingsLayer::create(nullptr, 0);
        l->show();
        KeybindingsLayer::create(l)->show();
    } else if (name == "videoadv") {
        MoreVideoOptionsLayer::create()->show();
    } else if (name.rfind("opt", 0) == 0) {
        // optAccount, optHelp, ...: the Settings dropdown, then its button
        auto l = OptionsLayer::create();
        scene->addChild(l, 100);
        l->showLayer(true);
        auto what = name.substr(3);
        if (what == "Account") l->onAccount(nullptr);
        else if (what == "Help") l->onHelp(nullptr);
        else if (what == "Video") l->onVideo(nullptr);
        else if (what == "Parental") l->onParental(nullptr);
        else if (what == "Support") l->onSupport(nullptr);
        else if (what == "Songs") l->onSoundtracks(nullptr);
        else if (what == "Vault") l->onSecretVault(nullptr);
    } else if (name == "call:all") {
        // Every server action the game has, with made-up ids, so the request recorder gets
        // their exact bodies. Only with CADENCE_HTTP_REDIRECT (a fake-account home).
        if (std::getenv("CADENCE_HTTP_REDIRECT")) {
            auto glm = GameLevelManager::sharedState();
            glm->uploadAccountComment("Hello from Cadence");
            glm->uploadLevelComment(10565740, "Great level", 100);
            glm->deleteLevelComment(901, 10565740);
            glm->likeItem(LikeItemType::Level, 10565740, true, 0);
            glm->likeItem(LikeItemType::Comment, 902, false, 10565740);
            glm->likeItem(LikeItemType::AccountComment, 903, true, 21);
            glm->rateStars(10565741, 7);
            glm->rateDemon(10565742, 3, false);
            glm->reportLevel(10565743);
            glm->uploadFriendRequest(21, "hi from Cadence");
            glm->acceptFriendRequest(22, 501);
            glm->deleteSentFriendRequest(23);
            glm->removeUserFromList(24, UserListType::Friends);
            glm->removeUserFromList(25, UserListType::Blocked);
            glm->uploadUserMessage(26, "Subject", "Body text");
        }
    } else if (name == "call:upload") {
        // a made-up level uploaded to the recorder (uploadGJLevel21.php's exact body)
        if (std::getenv("CADENCE_HTTP_REDIRECT")) {
            auto level = GJGameLevel::create();
            level->m_levelName = "Cadence Upload";
            level->m_levelDesc = "A test";
            level->m_levelString = "H4sIAAAAAAAAC6WQwQ3DMAhFF3IlPmBslFNm6AAMkBU6fG3jXqJWStULD54_Fpj3bdj2ztHNnl3h5o0M5uSCNzGVnSvOrcUOPOfcAhNs5tYF2VW5brGqsJQLpi_1wTxa6nY7_75OlqvPvwM2bX3RJE1Eu3K1cgr2PL4nM_Ns1Ai2rCrX9I3l2V1G3P2bz5JqbOk9gObNGcHKAQAA";
            level->m_audioTrack = 0;
            level->m_songID = 0;
            level->m_levelVersion = 1;
            level->m_levelLength = 0;
            level->m_coins = 0;
            level->m_starsRequested = 0;
            level->m_workingTime = 120;
            level->m_workingTime2 = 60;
            level->m_isEditable = true;
            level->m_levelType = GJLevelType::Editor;
            GameLevelManager::sharedState()->uploadLevel(level);
        }
    } else if (name == "call:more") {
        // the rest (getUserList's binding is wrong on this build: it stopped the first batch)
        if (std::getenv("CADENCE_HTTP_REDIRECT")) {
            auto glm = GameLevelManager::sharedState();
            glm->getAccountComments(27, 0, 0);
            glm->updateDescription(10565744, "A description");
            glm->getGJRewards(0);
            glm->getGJChallenges();
            glm->getLeaderboardScores(LeaderboardType::Default, LeaderboardStat::Stars);
            auto level = GJGameLevel::create();
            level->m_levelID = 10565745;
            glm->getLevelLeaderboard(level, LevelLeaderboardType::Global, LevelLeaderboardMode::Time);
            glm->getLevelLeaderboard(level, LevelLeaderboardType::Friends, LevelLeaderboardMode::Points);
        }
    } else if (name.rfind("myprofile", 0) == 0) {
        // the logged-in menu's profile button (MenuLayer::onMyProfile): your own ProfilePage.
        // Fake-account homes only (re/home-fake) with CADENCE_HTTP_REDIRECT.
        if (std::getenv("CADENCE_HTTP_REDIRECT")) {
            if (auto menu = scene->getChildByType<MenuLayer>(0)) menu->onMyProfile(nullptr);
        }
        if (name.find("/then3label:") != std::string::npos) g_guiShotFrame = 300;
    } else if (name.rfind("acct", 0) == 0) {
        // acctRegister, acctHelp, acctMore, acctLogin: the Account page, then that button.
        // (Never Save/Load: run these with a logged-out or fake-account home and
        // CADENCE_HTTP_REDIRECT, so nothing can reach a real account.)
        auto l = AccountLayer::create();
        scene->addChild(l, 100);
        l->showLayer(true);
        auto what = name.substr(4);
        // The actions below send requests: only with the local recorder standing in for the
        // servers (CADENCE_HTTP_REDIRECT), never against the real ones.
        bool recorded = std::getenv("CADENCE_HTTP_REDIRECT") != nullptr;
        if (what == "Register") l->onRegister(nullptr);
        else if (what == "Help") l->onHelp(nullptr);
        else if (what == "More") l->onMore(nullptr);
        else if (what == "Login") l->onLogin(nullptr);
        else if (what == "Backup" && recorded) l->doBackup();
        else if (what == "Sync" && recorded) l->doSync();
        else if (what == "RegisterSubmit" && recorded) {
            // fill the register form with made-up values and press Submit (onSubmit 0x3f6678)
            l->onRegister(nullptr);
            for (auto c : CCArrayExt<CCNode*>(scene->getChildren())) {
                if (std::string(geode::cocos::getObjectName(c)) == "AccountRegisterLayer") {
                    auto r = static_cast<AccountRegisterLayer*>(c);
                    r->m_usernameField->setString("CadenceTest");
                    r->m_passwordField->setString("test-pass-123");
                    r->m_confirmPasswordField->setString("test-pass-123");
                    r->m_emailField->setString("test@example.com");
                    r->m_verifyEmailField->setString("test@example.com");
                    reinterpret_cast<void (*)(AccountRegisterLayer*, CCObject*)>(geode::base::get() + 0x3f6678)(r, nullptr);
                }
            }
        }
    } else if (name == "levels") {
        // press the main menu's Play button (its biggest item): LevelSelectLayer::create from the
        // bindings doesn't build the real layer on this platform
        if (auto menu = scene->getChildByType<MenuLayer>(0)) {
            CCMenuItemSpriteExtra* best = nullptr;
            float area = 0;
            std::function<void(CCNode*)> walk = [&](CCNode* n) {
                for (auto c : CCArrayExt<CCNode*>(n->getChildren())) {
                    if (auto item = typeinfo_cast<CCMenuItemSpriteExtra*>(c)) {
                        auto sz = item->getContentSize();
                        if (sz.width * sz.height > area) { area = sz.width * sz.height; best = item; }
                    }
                    walk(c);
                }
            };
            walk(menu);
            if (best) best->activate();
        }
    } else if (name == "stats" || name == "achievements") {
        // through the main menu's own buttons
        if (auto menu = scene->getChildByType<MenuLayer>(0)) {
            if (name == "stats") menu->onStats(nullptr);
            else menu->onAchievements(nullptr);
        }
    } else if (name == "colors") {
        CCDirector::get()->replaceScene(GJGarageLayer::scene());
        g_guiTab = 100;  // open the paint page once the kit is up
    } else if (name.rfind("garage", 0) == 0) {
        CCDirector::get()->replaceScene(GJGarageLayer::scene());
        g_guiTab = name.size() > 6 ? std::atoi(name.c_str() + 6) : -1;
    } else if (name == "creator") {
        CCDirector::get()->replaceScene(CreatorLayer::scene());
    } else if (name == "shards") {
        CCDirector::get()->replaceScene(GJGarageLayer::scene());
        g_guiTab = 101;  // open the shards page once the kit is up
    } else if (name == "towerinner") {
        CCDirector::get()->replaceScene(LevelAreaInnerLayer::scene(false));
    } else if (name.rfind("shop", 0) == 0) {
        CCDirector::get()->replaceScene(GJShopLayer::scene(static_cast<ShopType>(std::atoi(name.c_str() + 4))));
    } else if (name == "secret1") {
        CCDirector::get()->replaceScene(SecretLayer::scene());
    } else if (name == "secret2") {
        CCDirector::get()->replaceScene(SecretLayer2::scene());
    } else if (name == "secret3") {
        CCDirector::get()->replaceScene(SecretLayer3::scene());
    } else if (name == "secret4") {
        CCDirector::get()->replaceScene(SecretLayer4::scene());
    } else if (name == "secretrewards") {
        CCDirector::get()->replaceScene(SecretRewardsLayer::scene(false));
    } else if (name.rfind("garage:", 0) == 0) {
        CCDirector::get()->replaceScene(GJGarageLayer::scene());
        g_guiItem = std::atoi(name.c_str() + 7);
        g_guiMore.clear();
        g_guiShotFrame = 300;
    } else if (name.rfind("sharelevel", 0) == 0) {
        // a verified editor level's page, then its Share button (EditLevelLayer::onShare):
        // ShareLevelLayer. Fake-account homes with CADENCE_HTTP_REDIRECT.
        auto level = GJGameLevel::create();
        level->m_levelType = GJLevelType::Editor;
        level->m_levelName = "Cadence Upload";
        level->m_isVerified = 1;
        level->m_isVerifiedRaw = true;
        level->m_objectCount = 12;
        level->m_levelString = "H4sIAAAAAAAAC6WQwQ3DMAhFF3IlPmBslFNm6AAMkBU6fG3jXqJWStULD54_Fpj3bdj2ztHNnl3h5o0M5uSCNzGVnSvOrcUOPOfcAhNs5tYF2VW5brGqsJQLpi_1wTxa6nY7_75OlqvPvwM2bX3RJE1Eu3K1cgr2PL4nM_Ns1Ai2rCrX9I3l2V1G3P2bz5JqbOk9gObNGcHKAQAA";
        auto scene2 = EditLevelLayer::scene(level);
        CCDirector::get()->replaceScene(scene2);
        // (its Share button is pressed later with /thenframe:GJ_shareBtn_001.png, once the page
        // is the running scene: a popup opened earlier lands on the scene being replaced)
    } else if (name.rfind("editlevel:", 0) == 0) {
        // editlevel:N[/M...]: a fresh editor level's page (EditLevelLayer), then presses as
        // creator: does (N = 0 is its Edit button: the editor)
        auto level = GJGameLevel::create();
        level->m_levelType = GJLevelType::Editor;
        CCDirector::get()->replaceScene(EditLevelLayer::scene(level));
        g_guiItem = std::atoi(name.c_str() + 10);
        g_guiMore.clear();
        for (size_t at = name.find('/'); at != std::string::npos; at = name.find('/', at + 1)) {
            if (name.compare(at + 1, 5, "edit:") == 0) break;
            g_guiMore.push_back(std::atoi(name.c_str() + at + 1));
        }
        g_guiShotFrame = 400 + 150 * int(g_guiMore.size()) + (name.find("/edit:") != std::string::npos ? 100 : 0);
    } else if (name.rfind("creator:", 0) == 0) {
        CCDirector::get()->replaceScene(CreatorLayer::scene());
        g_guiItem = std::atoi(name.c_str() + 8);
        g_guiMore.clear();
        for (size_t at = name.find('/'); at != std::string::npos; at = name.find('/', at + 1)) {
            if (name.compare(at + 1, 9, "dumpbuild") == 0 || name.compare(at + 1, 5, "edit:") == 0) break;
            g_guiMore.push_back(std::atoi(name.c_str() + at + 1));
        }
        g_guiShotFrame = 400 + 150 * int(g_guiMore.size()) + (name.find("/edit:") != std::string::npos ? 100 : 0);
    } else if (name.rfind("menu:", 0) == 0) {
        g_guiItem = std::atoi(name.c_str() + 5);
        g_guiShotFrame = 300;
    } else if (name.rfind("search", 0) == 0 && name.rfind("searchx", 0) != 0) {
        CCDirector::get()->replaceScene(LevelSearchLayer::scene(0));
    }
}

// The display's notch area: on a notched screen the full-screen viewport's top rows are never
// drawn (33 on a 14" MacBook). Returns how many leading rows are blank.
int blankTopRows(std::vector<unsigned char> const& rgba, int w, int h) {
    for (int y = 0; y < h; y++) {
        auto row = &rgba[size_t(y) * w * 4];
        for (int x = 0; x < w * 4; x++) if (row[x]) return y;
    }
    return 0;
}

void dumpScene(FILE* out, CCNode* node, int depth, std::map<std::tuple<void*, int, int, int, int>, std::string> const& names);

void dumpTriggerKeys(std::string const& path);

void saveFrame(std::string const& path) {
    GLint vp[4];
    glGetIntegerv(GL_VIEWPORT, vp);
    int w = vp[2], h = vp[3];
    std::vector<unsigned char> pixels(size_t(w) * h * 4), flipped(pixels.size());
    glPixelStorei(GL_PACK_ALIGNMENT, 1);
    glReadPixels(vp[0], vp[1], w, h, GL_RGBA, GL_UNSIGNED_BYTE, pixels.data());
    for (int y = 0; y < h; y++) {
        std::memcpy(&flipped[size_t(y) * w * 4], &pixels[size_t(h - 1 - y) * w * 4], size_t(w) * 4);
    }
    int top = blankTopRows(flipped, w, h);
    h -= top;
    auto image = new CCImage();
    image->initWithImageData(flipped.data() + size_t(top) * w * 4, int(size_t(w) * h * 4), CCImage::kFmtRawData, w, h, 8);
    image->saveToFile(path.c_str(), false);
    image->release();
    if (FILE* scene = std::fopen((path + ".scene").c_str(), "w")) {
        dumpScene(scene, CCDirector::get()->getRunningScene(), 0, frameNames());
        std::fclose(scene);
    }
    dumpTriggerKeys(path);
}

/// A trigger setup popup on top: which property key each input, toggle and slider edits
/// (`SetupTriggerPopup`'s m_inputNodes / m_valueToggles / m_valueControls), with world
/// positions and sizes, the slider ranges and the values shown. Written to <path>.keys.
void dumpTriggerKeys(std::string const& path) {
    auto scene = CCDirector::get()->getRunningScene();
    if (!scene) return;
    SetupTriggerPopup* popup = nullptr;
    for (auto c : CCArrayExt<CCNode*>(scene->getChildren())) {
        if (auto p = typeinfo_cast<SetupTriggerPopup*>(c)) popup = p;
    }
    if (!popup) return;
    FILE* out = std::fopen((path + ".keys").c_str(), "w");
    if (!out) return;
    auto number = [](CCObject* o) -> std::string {
        if (auto f = typeinfo_cast<CCFloat*>(o)) return fmt::format("{}", f->getValue());
        if (auto i = typeinfo_cast<CCInteger*>(o)) return fmt::format("{}", i->getValue());
        if (auto s = typeinfo_cast<CCString*>(o)) return s->getCString();
        return "?";
    };
    auto nodes = [&](char const* kind, CCDictionary* d) {
        if (!d) return;
        for (auto [key, obj] : CCDictionaryExt<intptr_t, CCObject*>(d)) {
            auto n = typeinfo_cast<CCNode*>(obj);
            if (!n) continue;
            auto p = n->convertToWorldSpaceAR({0, 0});
            auto sz = n->getScaledContentSize();
            std::fprintf(out, "%s %d %.2f %.2f %.2f %.2f %d %s\n", kind, int(key), p.x, p.y, sz.width, sz.height, n->isVisible() ? 1 : 0, typeid(*n).name());
        }
    };
    auto values = [&](char const* kind, CCDictionary* d) {
        if (!d) return;
        for (auto [key, obj] : CCDictionaryExt<intptr_t, CCObject*>(d)) std::fprintf(out, "%s %d %s\n", kind, int(key), number(obj).c_str());
    };
    nodes("input", popup->m_inputNodes);
    nodes("toggle", popup->m_valueToggles);
    nodes("control", popup->m_valueControls);
    nodes("label", popup->m_inputLabels);
    values("min", popup->m_minSliderValues);
    values("max", popup->m_maxSliderValues);
    values("value", popup->m_triggerValues);
    std::fprintf(out, "object %d page %d\n", popup->m_gameObject ? popup->m_gameObject->m_objectID : 0, popup->m_page);
    std::fclose(out);
}

/// The first-launch terms notice (a plain FLAlertLayer, shown a moment after the menu) would
/// cover the menus; layers derived from FLAlertLayer (the options pages) stay.
void removeNotices() {
    auto scene = CCDirector::get()->getRunningScene();
    if (!scene) return;
    std::vector<CCNode*> alerts;
    for (auto child : CCArrayExt<CCNode*>(scene->getChildren())) {
        // only the plain notice: every screen opened on purpose is a subclass
        if (typeid(*child) == typeid(FLAlertLayer)) alerts.push_back(child);
    }
    for (auto a : alerts) a->removeFromParent();
}

/// Press the visible button whose sprite shows `want` (the newest top-level layer's).
void pressFrame(std::string const& want) {
        // .../thenframe:<frame>: press the visible button whose sprite shows <frame> (safer than
        // an index: the walk order counts hidden items)

        auto names = frameNames();
        auto frameOf = [&](CCNode* nd) -> std::string {
            if (auto sp = typeinfo_cast<CCSprite*>(nd); sp && sp->getTexture()) {
                auto r = sp->getTextureRect();
                auto it = names.find({sp->getTexture(), int(r.origin.x * 4), int(r.origin.y * 4), int(r.size.width * 4), int(r.size.height * 4)});
                if (it != names.end()) return it->second;
            }
            return "";
        };
        CCMenuItemSpriteExtra* hit = nullptr;
        std::function<void(CCNode*)> walk = [&](CCNode* nd) {
            for (auto c : CCArrayExt<CCNode*>(nd->getChildren())) {
                if (hit || !c->isVisible()) continue;
                if (auto item = typeinfo_cast<CCMenuItemSpriteExtra*>(c)) {
                    std::function<bool(CCNode*)> shows = [&](CCNode* n2) {
                        if (frameOf(n2) == want) return true;
                        for (auto k : CCArrayExt<CCNode*>(n2->getChildren())) if (shows(k)) return true;
                        return false;
                    };
                    if (shows(item)) { hit = item; return; }
                }
                walk(c);
            }
        };
        auto scene = CCDirector::get()->getRunningScene();
        auto kids = scene ? scene->getChildren() : nullptr;
        if (kids && kids->count()) walk(static_cast<CCNode*>(kids->objectAtIndex(kids->count() - 1)));
        log::info("gui: then pressing the button showing {}: {}", want, hit ? "found" : "missing");
        if (hit) hit->activate();
    }

/// Press the visible button whose label reads `text` (ButtonSprites on 9-slices).
void pressLabel(std::string const& text) {
    CCMenuItemSpriteExtra* hit = nullptr;
    std::function<bool(CCNode*)> shows = [&](CCNode* n) {
        if (auto l = typeinfo_cast<CCLabelBMFont*>(n); l && l->getString() && text == l->getString()) return true;
        for (auto k : CCArrayExt<CCNode*>(n->getChildren())) if (shows(k)) return true;
        return false;
    };
    std::function<void(CCNode*)> walk = [&](CCNode* nd) {
        for (auto c : CCArrayExt<CCNode*>(nd->getChildren())) {
            if (hit || !c->isVisible()) continue;
            if (auto item = typeinfo_cast<CCMenuItemSpriteExtra*>(c); item && shows(item)) { hit = item; return; }
            walk(c);
        }
    };
    auto scene = CCDirector::get()->getRunningScene();
    auto kids = scene ? scene->getChildren() : nullptr;
    if (kids && kids->count()) walk(static_cast<CCNode*>(kids->objectAtIndex(kids->count() - 1)));
    log::info("gui: then pressing the button reading {}: {}", text, hit ? "found" : "missing");
    if (hit) hit->activate();
}

/// One frame of the gui sequence (called before each present).
void stepGui() {
    if (g_guiFrame < 0 || g_guiIndex >= g_spec.guis.size()) return;
    auto& [name, path] = g_spec.guis[g_guiIndex];
    g_guiFrame++;
    int lead = 0;
    if (g_guiFrame == lead + 1) {
        CCDirector::get()->replaceScene(MenuLayer::scene(false));
    }
    else if (g_guiFrame == lead + 38) removeNotices();
    else if (g_guiFrame == lead + 40) openGui(name);
    else if (g_guiFrame == lead + 90 && g_guiTab >= 0) {
        if (auto garage = CCDirector::get()->getRunningScene()->getChildByType<GJGarageLayer>(0)) {
            // GJGarageLayer::onPaint (0x2e4cb0; not exported here)
            if (g_guiTab == 100) reinterpret_cast<void (*)(GJGarageLayer*, CCObject*)>(geode::base::get() + 0x2e4cb0)(garage, nullptr);
            else if (g_guiTab == 101) garage->onShards(nullptr);
            else garage->selectTab(static_cast<IconType>(g_guiTab));
        }
        g_guiTab = -1;
    }
    else if (g_guiFrame == lead + 90 && g_guiItem >= 0) {
        // press the Nth CCMenuItemSpriteExtra of the running scene's first layer, depth first
        std::vector<CCMenuItemSpriteExtra*> items;
        std::function<void(CCNode*)> walk = [&](CCNode* n) {
            for (auto c : CCArrayExt<CCNode*>(n->getChildren())) {
                if (auto item = typeinfo_cast<CCMenuItemSpriteExtra*>(c)) items.push_back(item);
                walk(c);
            }
        };
        auto scene = CCDirector::get()->getRunningScene();
        if (scene && scene->getChildrenCount()) walk(static_cast<CCNode*>(scene->getChildren()->objectAtIndex(0)));
        log::info("gui: {} menu items, pressing {}", items.size(), g_guiItem);
        if (g_guiItem < int(items.size())) items[g_guiItem]->activate();
        g_guiItem = -1;
    }
    else if (g_guiFrame == lead + g_guiShotFrame - 2 && name.find("/dumpbuild") != std::string::npos) {
        // the editor's build tabs: every CreateMenuItem's object id, tab by tab
        if (auto ui = CCDirector::get()->getRunningScene()->getChildByType<LevelEditorLayer>(0) ? CCDirector::get()->getRunningScene()->getChildByType<LevelEditorLayer>(0)->m_editorUI : nullptr) {
            if (FILE* out = std::fopen((path + ".build").c_str(), "w")) {
                int t = 0;
                for (auto bar : CCArrayExt<EditButtonBar*>(ui->m_createButtonBars)) {
                    std::fprintf(out, "tab %d:", t++);
                    for (auto b : CCArrayExt<CCNode*>(bar->m_buttonArray)) {
                        if (std::strstr(typeid(*b).name(), "CreateMenuItem")) std::fprintf(out, " %d", static_cast<CreateMenuItem*>(b)->m_objectID);
                        else std::fprintf(out, " ?");
                    }
                    std::fprintf(out, "\n");
                }
                std::fclose(out);
            }
        }
    }
    else if (g_guiFrame == lead + g_guiShotFrame - 80 && name.find("/edit:") != std::string::npos) {
        // .../edit:<group|object|special>:<object id>: in the editor, place the object, select
        // it and open its Edit Group / Edit Object / Edit Special popup
        auto spec = name.substr(name.find("/edit:") + 6);
        if (auto t = spec.find("/then"); t != std::string::npos) spec = spec.substr(0, t);
        auto colon = spec.find(':');
        std::string kind = spec.substr(0, colon);
        int id = colon == std::string::npos ? 1 : std::atoi(spec.c_str() + colon + 1);
        if (auto lel = CCDirector::get()->getRunningScene()->getChildByType<LevelEditorLayer>(0)) {
            auto ui = lel->m_editorUI;
            auto obj = lel->createObject(id, {165.f, 105.f}, false);
            ui->selectObject(obj, true);
            log::info("gui: edit {} on object {}", kind, id);
            if (kind == "group") ui->editGroup(nullptr);
            else if (kind == "object") ui->editObject(nullptr);
            else if (kind == "special") ui->editObject2(nullptr);
            else if (kind == "settings") ui->onSettings(nullptr);
            else if (kind == "pause") ui->onPause(nullptr);
        }
    }
    else if (g_guiFrame == lead + g_guiShotFrame - (name.find("/then3label:") != std::string::npos ? 120 : 40) && name.find("/thenframe:") != std::string::npos) {
        // .../thenframe:<frame>: press the visible button whose sprite shows <frame> (safer than
        // an index: the walk order counts hidden items)
        auto want = name.substr(name.find("/thenframe:") + 11);
        pressFrame(want.substr(0, want.find('/')));
    }
    else if (g_guiFrame == lead + g_guiShotFrame - 40 && name.find("/then3label:") != std::string::npos) {
        // a third press (three-step paths: a page, a notice's OK, a button)
        auto want = name.substr(name.find("/then3label:") + 12);
        pressLabel(want.substr(0, want.find('/')));
    }
    else if (g_guiFrame == lead + g_guiShotFrame - (name.find("/then3label:") != std::string::npos ? 80 : 15) && name.find("/then2label:") != std::string::npos) {
        auto want = name.substr(name.find("/then2label:") + 12);
        pressLabel(want.substr(0, want.find('/')));
    }
    else if (g_guiFrame == lead + g_guiShotFrame - 15 && name.find("/then2frame:") != std::string::npos) {
        // .../then2frame:<frame>: a second press, on what the first opened
        auto want = name.substr(name.find("/then2frame:") + 12);
        pressFrame(want.substr(0, want.find('/')));
    }
    else if (g_guiFrame == lead + g_guiShotFrame - 40 && name.find("/then:") != std::string::npos) {
        // .../edit:...:<id>/then:N: once the popup is up, press its Nth button (depth first)
        int n = std::atoi(name.c_str() + name.find("/then:") + 6);
        std::vector<CCMenuItemSpriteExtra*> items;
        std::function<void(CCNode*)> walk = [&](CCNode* nd) {
            for (auto c : CCArrayExt<CCNode*>(nd->getChildren())) {
                if (auto item = typeinfo_cast<CCMenuItemSpriteExtra*>(c)) items.push_back(item);
                walk(c);
            }
        };
        auto scene = CCDirector::get()->getRunningScene();
        auto kids = scene ? scene->getChildren() : nullptr;
        if (kids && kids->count()) walk(static_cast<CCNode*>(kids->objectAtIndex(kids->count() - 1)));
        log::info("gui: then pressing {} of {}", n, items.size());
        if (n < int(items.size())) items[n]->activate();
    }
    else if (!g_guiMore.empty() && g_guiFrame > lead + 90 && (g_guiFrame - lead - 90) % 150 == 0) {
        // the next press, on whatever layer is on top now (a popup over the scene, or the scene)
        std::vector<CCMenuItemSpriteExtra*> items;
        std::function<void(CCNode*)> walk = [&](CCNode* n) {
            for (auto c : CCArrayExt<CCNode*>(n->getChildren())) {
                if (auto item = typeinfo_cast<CCMenuItemSpriteExtra*>(c)) items.push_back(item);
                walk(c);
            }
        };
        auto scene = CCDirector::get()->getRunningScene();
        auto kids = scene ? scene->getChildren() : nullptr;
        if (kids && kids->count()) walk(static_cast<CCNode*>(kids->objectAtIndex(kids->count() - 1)));
        int n = g_guiMore.front();
        g_guiMore.erase(g_guiMore.begin());
        log::info("gui: {} menu items on top, pressing {}", items.size(), n);
        for (size_t i = 0; i < items.size(); i++) {
            auto p = items[i]->convertToWorldSpaceAR({0, 0});
            log::info("gui item {} at {:.1f},{:.1f}", i, p.x, p.y);
        }
        if (n < int(items.size())) items[n]->activate();
    }
    else if (g_guiFrame == lead + g_guiShotFrame - 1 && name.rfind("opt", 0) != 0) removeNotices();
    else if (g_guiFrame == lead + g_guiShotFrame) {
        g_guiShotFrame = 140;
        saveFrame(path);
        g_guiIndex++;
        g_guiFrame = 0;
        if (g_guiIndex >= g_spec.guis.size()) _exit(0);
    }
}

class $modify(TraceMenu, MenuLayer) {
    bool init() {
        if (!MenuLayer::init()) return false;
        if (g_active && !g_launched) {
            g_launched = true;
            // The loading screen replaces the scene with this menu after init returns,
            // so switch to the level on the next frame instead of from inside init.
            queueInMainThread([] {
                PlatformToolbox::toggleFullScreen(false, false, false);
                if (!g_spec.dumpObjects.empty()) {
                    dumpObjects(g_spec.dumpObjects);
                    _exit(0);
                }
                if (!g_spec.dumpOffsets.empty()) {
                    dumpOffsets(g_spec.dumpOffsets);
                    _exit(0);
                }
                if (!g_spec.dumpAchievements.empty()) {
                    dumpAchievements(g_spec.dumpAchievements);
                    _exit(0);
                }
                if (!g_spec.guis.empty()) {
                    g_guiFrame = 0;
                    return;
                }
                if (!g_spec.dumpSprites.empty() && g_spec.levelString.empty()) {
                    dumpSprites(g_spec.dumpSprites);
                    _exit(0);
                }
                startAttempt();
            });
        }
        return true;
    }
};

class $modify(TraceBase, GJBaseGameLayer) {
    void update(float dt) {
        if (!recording(this)) {
            GJBaseGameLayer::update(dt);
            return;
        }
        auto layer = this;
        for (int i = 0; i < g_spec.speed && recording(layer); i++) {
            g_loopIndex = i;
            g_updates++;
            GJBaseGameLayer::update(g_spec.dt);
        }
    }

    void processCommands(float dt, bool isHalfTick, bool isLastTick) {
        if (!recording(this)) {
            GJBaseGameLayer::processCommands(dt, isHalfTick, isLastTick);
            return;
        }
        if (!g_out) {
            g_out = std::fopen(attemptPath().c_str(), "w");
            std::fprintf(g_out, "# cadence-trace level=%d dt=%.9g section_factor=%.9g,%.9g\n", g_spec.level,
                g_spec.dt, m_sectionXFactor, m_sectionYFactor);
            auto win = CCDirector::get()->getWinSize();
            std::fprintf(g_out, "# win=%.9g,%.9g\n", win.width, win.height);
            // The first recorded update's place in its rendered frame (scheduler ticks come
            // before updates whose index in the frame is 0).
            std::fprintf(g_out, "# sched speed=%d index=%d\n", g_spec.speed, g_loopIndex);
            std::fprintf(g_out, "step,dt,x,y,yvel,rot,flags,speed,size,gravity,fx,fy,cam_x,cam_y,touching,rotating,rot_speed,rect_w,rect_h,inner_w,inner_h,scale,on_slope,was_on_slope,slope_vel,slope_yvel,colliding_slope,slope_angle,slope_top,rising,zoom,cam_off_y,f23c,dual_static,mode_h,fly_center,ground_t,cam_x2,cam_y2,f3154,ground1_y,ground2_y,min_portal_y,max_portal_y\n");
        }
        auto& inputs = g_spec.attempts[g_attempt];
        auto [begin, end] = inputs.equal_range(g_step);
        for (auto it = begin; it != end; ++it) {
            this->handleButton(it->second.down, it->second.button, true);
        }
        g_stepDt = dt;
        if (!g_spec.debugRect.empty() && g_step >= g_spec.debugRectFrom && g_step <= g_spec.debugRectTo && m_player1) {
            static FILE* icf = nullptr;
            if (!icf) icf = std::fopen((g_spec.debugRect + ".icon").c_str(), "w");
            std::fprintf(icf, "%d icon rot=%.9g player rot=%.9g dashing=%d actions=%d\n", g_step,
                m_player1->m_iconSprite->getRotation(), m_player1->getRotation(), int(m_player1->m_isDashing),
                int(m_player1->m_iconSprite->numberOfRunningActions()));
            std::fflush(icf);
        }
        if (g_spec.watchObj >= 0 && g_step >= g_spec.debugRectFrom && g_step <= g_spec.debugRectTo && !g_spec.debugRect.empty()) {
            for (auto obj : CCArrayExt<GameObject*>(m_objects)) {
                if (obj->m_uniqueID != g_spec.watchObj) continue;
                auto raw = reinterpret_cast<char*>(obj);
                float c[4]; std::memcpy(c, raw + 0x350, sizeof c);
                auto r = obj->getObjectRect();
                static FILE* wf = nullptr;
                if (!wf) wf = std::fopen((g_spec.debugRect + ".obj").c_str(), "w");
                std::fprintf(wf, "%d obj %d cached=%.9g,%.9g,%.9g,%.9g rect=%.9g,%.9g,%.9g,%.9g obb=%d dirty=%d,%d rot=%.9g active=%d\n", g_step,
                    obj->m_uniqueID, c[0], c[1], c[2], c[3], r.origin.x, r.origin.y, r.size.width, r.size.height,
                    int(raw[0x2e8]), int(raw[0x360]), int(raw[0x361]), obj->getRotation(), int(raw[0x3f8]));
                std::fflush(wf);
            }
        }
        if (g_step == 0 && !g_spec.dumpLevel.empty()) dumpLevelObjects(m_objects, g_spec.dumpLevel);
        if (g_step == 0 && !g_spec.dumpSprites.empty()) {
            std::vector<GameObject*> objects;
            for (auto obj : CCArrayExt<GameObject*>(m_objects)) objects.push_back(obj);
            dumpSpritesOf(objects, g_spec.dumpSprites);
        }
        GJBaseGameLayer::processCommands(dt, isHalfTick, isLastTick);
    }

    // Runs once per physics step, after collisions: the end of the step.
    void checkRepellPlayer() {
        GJBaseGameLayer::checkRepellPlayer();
        if (!recording(this) || !g_out) return;
        record(m_player1, g_stepDt);
        // Dual mode (GJBaseGameLayer +0x3f2): player 2 goes to its own file, same columns.
        if (*reinterpret_cast<bool*>(reinterpret_cast<char*>(this) + 0x3f2) && m_player2) {
            if (!g_out2) {
                g_out2 = std::fopen((attemptPath() + ".p2").c_str(), "w");
                std::fprintf(g_out2, "# cadence-trace player 2\n");
            }
            record(m_player2, g_stepDt, g_out2);
        }
        if (g_step < 3 || (g_step >= 99 && g_step < 102)) dumpRaw(m_player1);
        for (auto& [step, path] : g_spec.particles) {
            if (step == g_step) dumpParticles(this, path, g_step);
        }
        for (auto& [step, path] : g_spec.drawNodes) {
            if (step == g_step) dumpDrawNodes(this, path, g_step);
        }
        for (auto& [group, path] : g_spec.watchGroups) watchGroup(this, group, path, g_step);
        if (++g_step >= g_spec.steps) finish("steps");
    }
};

int currentStep() { return g_step; }

FILE* triggerLog() {
    static FILE* f = nullptr;
    if (!f && !g_spec.triggers.empty()) {
        f = std::fopen(g_spec.triggers.c_str(), "w");
        std::fprintf(f, "step,id,uid,x,y,player_x,player_y,group,move_x,move_y,duration,easing,rate,touch,spawn,multi\n");
    }
    return f;
}

// One line per step: the positions of a group's first objects (to check move/rotate actions).
void watchGroup(GJBaseGameLayer* layer, int group, std::string const& path, int step) {
    static std::map<std::string, FILE*> files;
    FILE*& f = files[path];
    if (!f) f = std::fopen(path.c_str(), "w");
    if (!f) return;
    std::fprintf(f, "%d", step);
    if (auto objects = layer->getStaticGroup(group)) {
        int n = 0;
        for (auto obj : CCArrayExt<GameObject*>(objects)) {
            auto p = obj->getPosition();
            auto r = obj->getObjectRect();
            auto raw = reinterpret_cast<char*>(obj);
            double rx, ry; std::memcpy(&rx, raw + 0x3b0, 8); std::memcpy(&ry, raw + 0x3b8, 8);
            std::fprintf(f, ",%d:%.9g:%.9g:%.9g:rect=%.9g:%.9g:real=%.9g:%.9g", obj->m_objectID, p.x, p.y,
                obj->getRotation(), r.origin.x, r.origin.y, rx, ry);
            if (++n >= g_spec.watchMax) break;
        }
    }
    std::fprintf(f, "\n");
    std::fflush(f);
}

class $modify(TraceTrigger, EffectGameObject) {
    void triggerObject(GJBaseGameLayer* layer, int uniqueID, std::vector<int> const* remapKeys) {
        if (FILE* f = triggerLog(); f && layer && layer == PlayLayer::get()) {
            auto p = layer->m_player1 ? layer->m_player1->getPosition() : CCPoint{0, 0};
            std::fprintf(f, "%d,%d,%d,%.9g,%.9g,%.9g,%.9g,%d,%.9g,%.9g,%.9g,%d,%.9g,%d,%d,%d\n", currentStep(), m_objectID,
                uniqueID, getPositionX(), getPositionY(), p.x, p.y, m_targetGroupID, m_moveOffset.x, m_moveOffset.y,
                m_duration, int(m_easingType), m_easingRate, m_isTouchTriggered, m_isSpawnTriggered, m_isMultiTriggered);
            std::fflush(f);
        }
        EffectGameObject::triggerObject(layer, uniqueID, remapKeys);
    }
};

// Every particle system under `node`: position in the object layer's space plus its settings
// (appended to `out`, one CSV row each).
void dumpParticlesUnder(FILE* out, CCNode* node, CCNode* objectLayer, int step) {
    if (auto ps = typeinfo_cast<CCParticleSystem*>(node)) {
        auto parent = ps->getParent();
        auto world = parent ? parent->convertToWorldSpace(ps->getPosition()) : ps->getPosition();
        auto at = objectLayer->convertToNodeSpace(world);
        auto c = [](ccColor4F k) { return fmt::format("{:.4f}/{:.4f}/{:.4f}/{:.4f}", k.r, k.g, k.b, k.a); };
        int mode = ps->getEmitterMode();
        std::string modeData = mode == kCCParticleModeGravity
            ? fmt::format("grav={:.3f}/{:.3f} speed={:.3f}/{:.3f} radial={:.3f}/{:.3f} tangential={:.3f}/{:.3f}",
                ps->getGravity().x, ps->getGravity().y, ps->getSpeed(), ps->getSpeedVar(), ps->getRadialAccel(),
                ps->getRadialAccelVar(), ps->getTangentialAccel(), ps->getTangentialAccelVar())
            : fmt::format("radius={:.3f}/{:.3f}->{:.3f}/{:.3f} rps={:.3f}/{:.3f}", ps->getStartRadius(),
                ps->getStartRadiusVar(), ps->getEndRadius(), ps->getEndRadiusVar(), ps->getRotatePerSecond(),
                ps->getRotatePerSecondVar());
        std::fprintf(out, "%d,%.3f,%.3f,%s,%d,%d,%d,%d,%d,%.4f,%.4f,%.4f,%.3f,%.3f,%.3f/%.3f,%.3f/%.3f,%.3f/%.3f,%s,%s,%s,%s,%.3f,%.3f,%.3f,%d,%d,%d,%d,%s\n",
            step, at.x, at.y, typeid(*parent).name(), ps->getZOrder(), mode, int(ps->getPositionType()),
            int(ps->getTotalParticles()), int(ps->getParticleCount()), ps->getDuration(), ps->getLife(), ps->getLifeVar(),
            ps->getAngle(), ps->getAngleVar(), ps->getPosVar().x, ps->getPosVar().y, ps->getStartSize(), ps->getStartSizeVar(),
            ps->getEndSize(), ps->getEndSizeVar(), c(ps->getStartColor()).c_str(), c(ps->getStartColorVar()).c_str(),
            c(ps->getEndColor()).c_str(), c(ps->getEndColorVar()).c_str(), ps->getEmissionRate(), ps->getScaleX(),
            ps->getRotation(), ps->isActive(), ps->isVisible(), int(ps->getBlendFunc().src), int(ps->getBlendFunc().dst),
            modeData.c_str());
    }
    for (auto child : CCArrayExt<CCNode*>(node->getChildren())) dumpParticlesUnder(out, child, objectLayer, step);
}

// Every CCDrawNode (HardStreak trails...) under a node: its world transform and its triangle
// vertices (node space) with colors, one line per vertex.
void dumpDrawNodesUnder(FILE* out, CCNode* node, int step) {
    if (auto dn = typeinfo_cast<CCDrawNode*>(node)) {
        auto t = dn->nodeToWorldTransform();
        std::fprintf(out, "# step %d node %s z=%d vis=%d op=%d blend=%d,%d count=%d world=%.4f,%.4f,%.4f,%.4f,%.4f,%.4f pos=%.3f,%.3f\n",
            step, typeid(*dn).name(), dn->getZOrder(), dn->isVisible(), int(typeinfo_cast<CCNodeRGBA*>(node) ? 0 : -1),
            int(dn->m_sBlendFunc.src), int(dn->m_sBlendFunc.dst), int(dn->m_nBufferCount),
            t.a, t.b, t.c, t.d, t.tx, t.ty, dn->getPositionX(), dn->getPositionY());
        for (int i = 0; i < dn->m_nBufferCount; i++) {
            auto& v = dn->m_pBuffer[i];
            std::fprintf(out, "%.4f,%.4f,%d,%d,%d,%d\n", v.vertices.x, v.vertices.y, v.colors.r, v.colors.g, v.colors.b, v.colors.a);
        }
    }
    if (auto ms = typeinfo_cast<CCMotionStreak*>(node)) {
        auto t = ms->nodeToWorldTransform();
        auto c = ms->getColor();
        std::fprintf(out, "# step %d node CCMotionStreak z=%d vis=%d op=%d col=%d,%d,%d stroke=%.3f fade=%.4f minseg=%.3f maxseg=%.3f points=%u stroking=%d fast=%d world=%.4f,%.4f,%.4f,%.4f,%.4f,%.4f pos=%.3f,%.3f\n",
            step, ms->getZOrder(), ms->isVisible(), int(ms->getOpacity()), c.r, c.g, c.b, ms->m_fStroke, ms->m_fFadeDelta,
            ms->m_fMinSeg, ms->getM_fMaxSeg(), ms->m_uNuPoints, int(ms->m_bStroke), int(ms->m_bFastMode),
            t.a, t.b, t.c, t.d, t.tx, t.ty, ms->getPositionX(), ms->getPositionY());
        for (unsigned i = 0; i < ms->m_uNuPoints * 2; i++) {
            auto* col = ms->m_pColorPointer + i * 4;
            std::fprintf(out, "%.4f,%.4f,%d,%d,%d,%d,%.4f,%.4f\n", ms->m_pVertices[i].x, ms->m_pVertices[i].y,
                col[0], col[1], col[2], col[3], ms->m_pTexCoords[i].u, ms->m_pTexCoords[i].v);
        }
        for (unsigned i = 0; i < ms->m_uNuPoints; i++) {
            std::fprintf(out, "p %.4f,%.4f,%.4f\n", ms->m_pPointVertexes[i].x, ms->m_pPointVertexes[i].y, ms->m_pPointState[i]);
        }
    }
    for (auto child : CCArrayExt<CCNode*>(node->getChildren())) dumpDrawNodesUnder(out, child, step);
}

void dumpDrawNodes(GJBaseGameLayer* layer, std::string const& path, int step) {
    FILE* out = std::fopen(path.c_str(), "a");
    if (!out) return;
    dumpDrawNodesUnder(out, layer, step);
    std::fclose(out);
}

void dumpParticles(GJBaseGameLayer* layer, std::string const& path, int step) {
    FILE* out = std::fopen(path.c_str(), "a");
    // the per-particle-type limit createParticle checks (GJBaseGameLayer +0x2ff0, m1)
    if (out) std::fprintf(out, "# particle limit %u\n", *reinterpret_cast<unsigned*>(reinterpret_cast<char*>(layer) + 0x2ff0));
    if (!out) return;
    if (std::ftell(out) == 0) {
        std::fprintf(out, "step,x,y,parent,z,mode,postype,total,count,duration,life,lifevar,angle,anglevar,posvar,size,endsize,startcolor,startcolorvar,endcolor,endcolorvar,rate,scale,rot,active,visible,src,dst,modedata\n");
    }
    dumpParticlesUnder(out, layer, layer->m_objectLayer, step);
    std::fclose(out);
    // The object layer's direct children (batch layers, particle layers, the player), for z order.
    if (FILE* layers = std::fopen((path + ".layers").c_str(), "w")) {
        for (auto child : CCArrayExt<CCNode*>(layer->m_objectLayer->getChildren())) {
            std::fprintf(layers, "%s z=%d tag=%d kids=%d\n", typeid(*child).name(), child->getZOrder(), child->getTag(),
                child->getChildrenCount());
        }
        std::fclose(layers);
    }
}

// The scene graph under a node, one line per node (batches with many children are summarized).
void dumpScene(FILE* out, CCNode* node, int depth, std::map<std::tuple<void*, int, int, int, int>, std::string> const& names) {
    auto sprite = typeinfo_cast<CCSprite*>(node);
    std::string frame = "-";
    if (sprite && sprite->getTexture()) {
        auto r = sprite->getTextureRect();
        auto it = names.find({sprite->getTexture(), int(r.origin.x * 4), int(r.origin.y * 4), int(r.size.width * 4), int(r.size.height * 4)});
        frame = it != names.end() ? it->second : "?";
        if (frame == "?") {
            // a whole image (a loose texture, a 9-slice's pieces): its file
            for (auto [key, tex] : CCDictionaryExt<std::string, CCTexture2D*>(CCTextureCache::sharedTextureCache()->m_pTextures)) {
                if (tex == sprite->getTexture()) {
                    auto slash = key.find_last_of('/');
                    frame = "tex:" + (slash == std::string::npos ? key : key.substr(slash + 1));
                    break;
                }
            }
        }
    }
    std::string extra;
    if (auto label = typeinfo_cast<CCLabelBMFont*>(node)) {
        std::string text = label->getString();
        for (auto& c : text) if (c == '\n') c = '|';
        std::string font = label->getFntFile();
        auto slash = font.find_last_of('/');
        extra = " font=" + (slash == std::string::npos ? font : font.substr(slash + 1)) + " text=" + text;
    }
    auto rgba = typeinfo_cast<CCRGBAProtocol*>(node);
    auto blend = typeinfo_cast<CCBlendProtocol*>(node);
    auto pos = node->getPosition();
    auto anchor = node->getAnchorPoint();
    auto size = node->getContentSize();
    int children = node->getChildrenCount();
    std::fprintf(out, "%*s%s tag=%d z=%d pos=%.3f,%.3f anchor=%.3f,%.3f size=%.3f,%.3f scale=%.4f,%.4f rot=%.3f vis=%d op=%d col=%d,%d,%d blend=%d,%d frame=%s kids=%d\n",
        depth * 2, "", typeid(*node).name(), node->getTag(), node->getZOrder(), pos.x, pos.y, anchor.x, anchor.y,
        size.width, size.height, node->getScaleX(), node->getScaleY(), node->getRotation(), node->isVisible(),
        rgba ? int(rgba->getOpacity()) : -1, rgba ? rgba->getColor().r : -1, rgba ? rgba->getColor().g : -1,
        rgba ? rgba->getColor().b : -1, blend ? int(blend->getBlendFunc().src) : -1, blend ? int(blend->getBlendFunc().dst) : -1,
        frame.c_str(), children);
    if (sprite && (sprite->isFlipX() || sprite->isFlipY())) std::fprintf(out, "%*s#flip %d %d\n", depth * 2, "", int(sprite->isFlipX()), int(sprite->isFlipY()));
    if (node->isIgnoreAnchorPointForPosition()) std::fprintf(out, "%*s#ignoreanchor\n", depth * 2, "");
    if (!extra.empty()) std::fprintf(out, "%*s#label%s\n", depth * 2, "", extra.c_str());
    if (children > 400 || depth > 14 || (depth > 6 && children > 60)) return;
    for (auto child : CCArrayExt<CCNode*>(node->getChildren())) dumpScene(out, child, depth + 1, names);
}

// While recording, every scheduler tick (one rendered frame, `speed` game updates) advances the
// cocos actions by the game time those updates cover (speed × dt) instead of wall-clock time,
// so sprite actions the physics reads back (the dash icon spin) are reproducible.
class $modify(TraceScheduler, CCScheduler) {
    void update(float dt) {
        if (g_active && g_layer && g_spec.benchSecs <= 0) dt = g_spec.dt * float(g_spec.speed);
        CCScheduler::update(dt);
    }
};

// Screenshots: read the framebuffer right before the game presents the frame.
// Bench: the wall-clock interval of every presented frame once the level runs.
std::vector<double> g_benchFrames;
std::chrono::steady_clock::time_point g_benchLast, g_benchStart;

void benchFrame() {
    auto now = std::chrono::steady_clock::now();
    if (g_benchFrames.empty() && g_benchStart.time_since_epoch().count() == 0) {
        g_benchStart = now;
    } else {
        g_benchFrames.push_back(std::chrono::duration<double, std::milli>(now - g_benchLast).count());
    }
    g_benchLast = now;
    if (std::chrono::duration<double>(now - g_benchStart).count() >= g_spec.benchSecs) {
        if (std::FILE* f = std::fopen(g_spec.benchOut.c_str(), "w")) {
            auto size = CCEGLView::get()->getFrameSize();
            std::fprintf(f, "# frame %g %g retina %g\n", size.width, size.height, CCDirector::get()->getContentScaleFactor());
            // the counters /usr/bin/time -l reports (it can't wrap a LaunchServices launch):
            // CPU seconds, seconds since the process started, peak footprint in bytes
            rusage ru{};
            getrusage(RUSAGE_SELF, &ru);
            rusage_info_v4 ri{};
            proc_pid_rusage(getpid(), RUSAGE_INFO_V4, (rusage_info_t*)&ri);
            mach_timebase_info_data_t tb{};
            mach_timebase_info(&tb);
            double wall = double(mach_absolute_time() - ri.ri_proc_start_abstime) * tb.numer / tb.denom / 1e9;
            std::fprintf(f, "# rusage user %.3f sys %.3f wall %.3f footprint %llu\n", ru.ru_utime.tv_sec + ru.ru_utime.tv_usec / 1e6,
                         ru.ru_stime.tv_sec + ru.ru_stime.tv_usec / 1e6, wall, (unsigned long long)ri.ri_lifetime_max_phys_footprint);
            for (double ms : g_benchFrames) std::fprintf(f, "%.3f\n", ms);
            std::fclose(f);
        }
        _exit(0);
    }
}

class $modify(TraceView, CCEGLView) {
    void swapBuffers() {
        if (g_active && g_spec.mute) {
            auto audio = FMODAudioEngine::get();
            if (audio->getBackgroundMusicVolume() != 0) audio->setBackgroundMusicVolume(0);
            if (audio->m_sfxVolume != 0) audio->setEffectsVolume(0);
        }
        // (after a few seconds of play, so loading isn't counted)
        if (g_active && g_spec.benchSecs > 0 && g_layer && g_layer->m_gameState.m_levelTime > 2.0) benchFrame();
        if (g_active && !g_spec.guis.empty()) stepGui();
        if (g_active && g_spec.pauseStep >= 0 && g_layer && g_pauseFrames < 0 && g_step >= g_spec.pauseStep) {
            PlayLayer::get()->pauseGame(false);
            g_pauseFrames = 0;
        }
        if (g_pauseFrames >= 0 && ++g_pauseFrames == 60) {
            saveFrame(g_spec.pausePath);
            _exit(0);
        }
        if (g_endFrames >= 0 && ++g_endFrames == 300) {
            saveFrame(g_spec.endPath);
            _exit(0);
        }
        if (g_active && g_layer && !g_spec.screenshots.empty() && g_step >= g_spec.screenshots.front().first) {
            auto [target, path] = g_spec.screenshots.front();
            g_spec.screenshots.erase(g_spec.screenshots.begin());
            GLint vp[4];
            glGetIntegerv(GL_VIEWPORT, vp);
            int w = vp[2], h = vp[3];
            std::vector<unsigned char> pixels(size_t(w) * h * 4), flipped(pixels.size());
            glPixelStorei(GL_PACK_ALIGNMENT, 1);
            glReadPixels(vp[0], vp[1], w, h, GL_RGBA, GL_UNSIGNED_BYTE, pixels.data());
            for (int y = 0; y < h; y++) {
                std::memcpy(&flipped[size_t(y) * w * 4], &pixels[size_t(h - 1 - y) * w * 4], size_t(w) * 4);
            }
            // drop the notch band (blank top rows); the .txt size is the frame's real size
            // The scene is laid out over the whole viewport (h rows); the notch hides the top
            // rows, which are cut from the image. The .txt keeps the full size and the cut.
            int top = blankTopRows(flipped, w, h);
            auto image = new CCImage();
            image->initWithImageData(flipped.data() + size_t(top) * w * 4, int(size_t(w) * (h - top) * 4), CCImage::kFmtRawData, w, h - top, 8);
            image->saveToFile(path.c_str(), false);
            image->release();
            if (FILE* scene = std::fopen((path + ".scene").c_str(), "w")) {
                dumpScene(scene, CCDirector::get()->getRunningScene(), 0, frameNames());
                std::fclose(scene);
            }
            if (FILE* meta = std::fopen((path + ".txt").c_str(), "w")) {
                auto cam = g_layer->m_gameState.m_cameraPosition;
                std::fprintf(meta, "step %d size %d %d camera %.9g %.9g player %.9g %.9g crop %d\n", g_step, w, h,
                    cam.x, cam.y, g_layer->m_player1->getPosition().x, g_layer->m_player1->getPosition().y, top);
                std::fclose(meta);
            }
        }
        CCEGLView::swapBuffers();
    }
};

class $modify(TracePlay, PlayLayer) {
    void destroyPlayer(PlayerObject* player, GameObject* object) {
        // bench: the player can't die (the start's anti-cheat check still goes through)
        if (g_active && (g_spec.benchSecs > 0 || g_spec.noclip) && object != m_anticheatSpike) return;
        PlayLayer::destroyPlayer(player, object);
        if (recording(this) && g_out && player == m_player1 && player->m_isDead) {
            record(player, g_stepDt);
            if (object) {
                // The object that killed the player (a comment line: readers skip it).
                auto r = object->getObjectRect();
                auto pos = object->getPosition();
                std::fprintf(g_out, "# killer id=%d type=%d pos=%.9g,%.9g rect=%.9g,%.9g,%.9g,%.9g rot=%.9g uid=%d\n",
                    object->m_objectID, int(object->m_objectType), pos.x, pos.y, r.origin.x, r.origin.y,
                    r.size.width, r.size.height, object->getRotation(), object->m_uniqueID);
            }
            finish("death");
        } else if (recording(this) && g_out && player == m_player2 && player->m_isDead && g_out2) {
            // Player 2 dying in dual mode ends the attempt too: both rows for this step.
            record(m_player1, g_stepDt);
            record(player, g_stepDt, g_out2);
            finish("death");
        }
    }

    void levelComplete() {
        if (recording(this) && !g_spec.endPath.empty()) {
            // the end screen: the real completion, at normal speed, captured in swapBuffers
            g_spec.speed = 1;
            g_endFrames = 0;
            PlayLayer::levelComplete();
            return;
        }
        if (recording(this)) {
            finish("complete");
            return;
        }
        PlayLayer::levelComplete();
    }
};

// Debug: the player's cached object rect (+0x350, dirty flags +0x360/+0x361) and m_width
// (+0x2fc) around solid collisions and mode switches, for steps in [from, to].
int currentStep();
static void logRect(PlayerObject* p, char const* what, int extra) {
    int step = currentStep();
    if (g_spec.debugRect.empty() || step < g_spec.debugRectFrom || step > g_spec.debugRectTo) return;
    static FILE* f = nullptr;
    if (!f) f = std::fopen(g_spec.debugRect.c_str(), "w");
    auto raw = reinterpret_cast<char*>(p);
    float r[4]; std::memcpy(r, raw + 0x350, sizeof r);
    float w; std::memcpy(&w, raw + 0x2fc, sizeof w);
    std::fprintf(f, "%d %s %d rect=%.9g,%.9g,%.9g,%.9g dirty=%d,%d width=%.9g rot=%.9g y=%.9g\n", step, what, extra,
        r[0], r[1], r[2], r[3], int(raw[0x360]), int(raw[0x361]), w, p->getRotation(), p->getPosition().y);
    std::fflush(f);
}

class $modify(DebugRectPlayer, PlayerObject) {
    void switchedToMode(GameObjectType type) {
        logRect(this, "before-switchedToMode", int(type));
        PlayerObject::switchedToMode(type);
        logRect(this, "after-switchedToMode", int(type));
    }
    void modeDidChange() {
        PlayerObject::modeDidChange();
        logRect(this, "after-modeDidChange", 0);
    }
    bool collidedWithObjectInternal(float dt, GameObject* object, cocos2d::CCRect rect, bool skipCheck) {
        logRect(this, "before-internal", object ? object->m_uniqueID : 0);
        logRect(this, "  caller", int(reinterpret_cast<uintptr_t>(__builtin_return_address(0)) - geode::base::get()));
        logRect(this, "  rect-x", int(rect.origin.x * 100));
        auto r = PlayerObject::collidedWithObjectInternal(dt, object, rect, skipCheck);
        logRect(this, "after-internal", object ? object->m_objectID : 0);
        return r;
    }
    void touchedObject(GameObject* object) {
        if (object) {
            auto r = object->getObjectRect();
            auto pos = this->getPosition();
            char buf[256];
            std::snprintf(buf, sizeof buf, "touched id=%d obj=%.9g,%.9g,%.9g,%.9g px=%.9g", object->m_objectID,
                r.origin.x, r.origin.y, r.size.width, r.size.height, pos.x);
            logRect(this, buf, object->m_objectID);
        }
        PlayerObject::touchedObject(object);
    }
    void ringJump(RingObject* object, bool skipCheck) {
        logRect(this, "ringJump", object ? object->m_uniqueID * 10000 + object->m_objectID : 0);
        PlayerObject::ringJump(object, skipCheck);
        logRect(this, "  after-ringJump dashing", int(this->m_isDashing));
    }
    void addToTouchedRings(RingObject* object) {
        logRect(this, "addToTouchedRings", object ? object->m_uniqueID * 10000 + object->m_objectID : 0);
        PlayerObject::addToTouchedRings(object);
    }
    bool pushButton(PlayerButton button) {
        logRect(this, "pushButton touching", this->m_touchingRings ? int(this->m_touchingRings->count()) : -1);
        return PlayerObject::pushButton(button);
    }
    bool collidedWithObject(float dt, GameObject* object) {
        logRect(this, "before-collide2", object ? object->m_objectID : 0);
        auto r = PlayerObject::collidedWithObject(dt, object);
        logRect(this, "after-collide2", object ? object->m_objectID : 0);
        return r;
    }
    bool collidedWithObject(float dt, GameObject* object, cocos2d::CCRect rect, bool skipCheck) {
        logRect(this, "before-collide", object ? object->m_objectID : 0);
        auto r = PlayerObject::collidedWithObject(dt, object, rect, skipCheck);
        logRect(this, "after-collide", object ? object->m_objectID : 0);
        return r;
    }
};

// Which search each online request is (gui runs): log the search objects' type and query.
class $modify(TraceSearchObject, GJSearchObject) {
    static GJSearchObject* create(SearchType type) {
        if (g_active && !g_spec.guis.empty()) log::info("search object: type {}", int(type));
        return GJSearchObject::create(type);
    }
    static GJSearchObject* create(SearchType type, std::string query) {
        if (g_active && !g_spec.guis.empty()) log::info("search object: type {} query '{}'", int(type), query);
        return GJSearchObject::create(type, query);
    }
    static GJSearchObject* create(SearchType type, std::string query, std::string difficulty, std::string length, int page, bool star, bool uncompleted, bool featured, int songID, bool original, bool twoPlayer, bool customSong, bool songFilter, bool noStar, bool coins, bool epic, bool legendary, bool mythic, bool onlyCompleted, int demonFilter, int folder, int searchMode) {
        if (g_active && !g_spec.guis.empty()) log::info("search object: type {} query '{}' diff '{}' len '{}' mode {}", int(type), query, difficulty, length, searchMode);
        return GJSearchObject::create(type, query, difficulty, length, page, star, uncompleted, featured, songID, original, twoPlayer, customSong, songFilter, noStar, coins, epic, legendary, mythic, onlyCompleted, demonFilter, folder, searchMode);
    }
};

// Every server request the game makes, for comparing Cadence's requests with the real game's
// (CADENCE_HTTP_LOG=<file>): url, tag, type and the form body, one per line, with credentials
// masked (gjp2/password values become their length). CADENCE_HTTP_REDIRECT=<http://host:port>
// sends the requests there instead of the game's server (a local recorder sees the headers).
namespace {
std::string maskCredentials(std::string params) {
    for (char const* key : {"gjp2=", "password=", "gjp="}) {
        size_t at = 0;
        while ((at = params.find(key, at)) != std::string::npos) {
            if (at != 0 && params[at - 1] != '&') { at += 1; continue; }
            size_t start = at + std::strlen(key);
            size_t end = params.find('&', start);
            if (end == std::string::npos) end = params.size();
            std::string masked = "<" + std::to_string(end - start) + " chars>";
            params.replace(start, end - start, masked);
            at = start + masked.size();
        }
    }
    return params;
}

std::string logRequest(char const* who, std::string url, std::string const& params, std::string const& tag, int type) {
    if (char const* path = std::getenv("CADENCE_HTTP_LOG")) {
        if (std::FILE* f = std::fopen(path, "a")) {
            std::fprintf(f, "%s\t%s\t%s\t%d\t%s\n", who, url.c_str(), tag.c_str(), type, maskCredentials(params).c_str());
            std::fclose(f);
        }
    }
    if (char const* to = std::getenv("CADENCE_HTTP_REDIRECT")) {
        // keep the path after the host (".../database/getGJLevels21.php")
        size_t scheme = url.find("://");
        size_t path = scheme == std::string::npos ? std::string::npos : url.find('/', scheme + 3);
        if (path != std::string::npos) url = std::string(to) + url.substr(path);
    }
    return url;
}
}

class $modify(HttpLogLevels, GameLevelManager) {
    void ProcessHttpRequest(std::string url, std::string params, std::string tag, GJHttpType type) {
        url = logRequest("GameLevelManager", url, params, tag, int(type));
        GameLevelManager::ProcessHttpRequest(url, params, tag, type);
    }
};

class $modify(HttpLogAccount, GJAccountManager) {
    void ProcessHttpRequest(std::string url, std::string params, std::string tag, GJHttpType type) {
        url = logRequest("GJAccountManager", url, params, tag, int(type));
        GJAccountManager::ProcessHttpRequest(url, params, tag, type);
    }
};
