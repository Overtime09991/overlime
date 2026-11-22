// OVERLIME core: schlanke, flache C-Schnittstelle um die chiaki-Bibliothek (fuer Rust-FFI).
#ifndef OVERLIME_CORE_H
#define OVERLIME_CORE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
	int target;               // ChiakiTarget
	int ps5;
	char nickname[0x20];
	uint8_t rp_regist_key[16];
	uint8_t rp_key[16];
	uint32_t rp_key_type;
} OlRegistered;

typedef struct {
	char host[128];
	int ps5;
	uint8_t rp_regist_key[16];
	uint8_t rp_key[16];
	int width, height, fps;
	int bitrate_kbps;         // 0 = Standard des Presets
	int codec;                // 0 = H.264, 1 = HEVC
	int hw_decode;            // 1 = D3D11VA versuchen
} OlStreamParams;

typedef struct {
	int state;                // 0 verbindet, 1 verbunden, 2 beendet
	int quit_error;           // 1 wenn Fehler
	char message[256];
	uint64_t frames_decoded;
	int width, height;
	uint32_t audio_rate, audio_channels;
	int32_t frames_lost;
	uint64_t audio_total;     // empfangene Ton-Samples (pro Kanal)
} OlStatus;

typedef struct OlStream OlStream;

void ol_init(void);
// Gesammelte Log-Zeilen holen und leeren. Rueckgabe: Anzahl Bytes.
size_t ol_log_take(char *buf, size_t buf_size);

// Blockierend. 0 = Erfolg, sonst Fehlercode; err bekommt Text.
int ol_register(const char *host, const char *system_version, int ps5,
		const uint8_t account_id[8], uint32_t pin, OlRegistered *out,
		char *err, size_t err_size, int timeout_ms);

OlStream *ol_stream_start(const OlStreamParams *params, char *err, size_t err_size);
void ol_stream_status(OlStream *s, OlStatus *out);
// 1 wenn ein neues Bild geholt wurde (RGBA, width*height*4). buf muss gross genug sein.
int ol_stream_take_frame(OlStream *s, uint8_t *buf, size_t buf_size, int *width, int *height);
// Interleaved int16 PCM; Rueckgabe: Anzahl int16-Samples
size_t ol_stream_take_audio(OlStream *s, int16_t *buf, size_t max_samples);
void ol_stream_set_input(OlStream *s, uint32_t buttons, uint8_t l2, uint8_t r2,
		int16_t lx, int16_t ly, int16_t rx, int16_t ry);
// Letzte Vibrationswerte der Konsole (0-255). Rueckgabe 1 wenn seit dem letzten Abruf neu.
int ol_stream_take_rumble(OlStream *s, uint8_t *left, uint8_t *right);
void ol_stream_stop(OlStream *s);

// Button-Masken (wie ChiakiControllerButton)
enum {
	OL_BTN_CROSS = (1 << 0), OL_BTN_MOON = (1 << 1), OL_BTN_BOX = (1 << 2), OL_BTN_PYRAMID = (1 << 3),
	OL_BTN_DPAD_LEFT = (1 << 4), OL_BTN_DPAD_RIGHT = (1 << 5), OL_BTN_DPAD_UP = (1 << 6), OL_BTN_DPAD_DOWN = (1 << 7),
	OL_BTN_L1 = (1 << 8), OL_BTN_R1 = (1 << 9), OL_BTN_L3 = (1 << 10), OL_BTN_R3 = (1 << 11),
	OL_BTN_OPTIONS = (1 << 12), OL_BTN_SHARE = (1 << 13), OL_BTN_TOUCHPAD = (1 << 14), OL_BTN_PS = (1 << 15)
};

#ifdef __cplusplus
}
#endif

#endif
