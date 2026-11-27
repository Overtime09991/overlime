#include "overlime_core.h"

#include <chiaki/remote/holepunch.h>
#include <chiaki/session.h>
#include <chiaki/regist.h>
#include <chiaki/discovery.h>
#include <chiaki/ffmpegdecoder.h>
#include <chiaki/opusdecoder.h>
#include <chiaki/opusencoder.h>
#include <opus/opus.h>
#include <chiaki/controller.h>
#include <chiaki/orientation.h>
#include <chiaki/videoreceiver.h>
#include <chiaki/log.h>
#include <chiaki/thread.h>
#include <chiaki/time.h>
#include <windows.h>

#include <libswscale/swscale.h>
#include <libavutil/imgutils.h>
#include <libavutil/hwcontext.h>
#include <libavutil/pixfmt.h>
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/channel_layout.h>
#include <libavutil/opt.h>
#include <libavutil/mathematics.h>
#include <libavutil/avutil.h>

#include <stdio.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>

static uint64_t now_us(void)
{
	LARGE_INTEGER f, t;
	QueryPerformanceFrequency(&f);
	QueryPerformanceCounter(&t);
	return (uint64_t)((double)t.QuadPart * 1000000.0 / (double)f.QuadPart);
}

// ---------------------------------------------------------------- Log
#define LOG_CAP 65536
static char g_log[LOG_CAP];
static size_t g_log_len;
static ChiakiMutex g_log_mutex;
static int g_inited;

static void log_cb(ChiakiLogLevel level, const char *msg, void *user)
{
	(void)user;
	const char *tag = level == CHIAKI_LOG_ERROR ? "E" : level == CHIAKI_LOG_WARNING ? "W" : level == CHIAKI_LOG_INFO ? "I" : "D";
	chiaki_mutex_lock(&g_log_mutex);
	int n = snprintf(g_log + g_log_len, LOG_CAP - g_log_len, "[%s] %s\n", tag, msg);
	if(n > 0 && g_log_len + (size_t)n < LOG_CAP)
		g_log_len += (size_t)n;
	chiaki_mutex_unlock(&g_log_mutex);
}

void ol_init(void)
{
	if(g_inited)
		return;
	g_inited = 1;
	chiaki_mutex_init(&g_log_mutex, false);
}

size_t ol_log_take(char *buf, size_t buf_size)
{
	if(!g_inited || buf_size == 0)
		return 0;
	chiaki_mutex_lock(&g_log_mutex);
	size_t n = g_log_len < buf_size - 1 ? g_log_len : buf_size - 1;
	memcpy(buf, g_log, n);
	buf[n] = 0;
	memmove(g_log, g_log + n, g_log_len - n);
	g_log_len -= n;
	chiaki_mutex_unlock(&g_log_mutex);
	return n;
}

static void make_log(ChiakiLog *log)
{
	chiaki_log_init(log, CHIAKI_LOG_ALL & ~CHIAKI_LOG_VERBOSE, log_cb, NULL);
}

// ---------------------------------------------------------- Registrierung
typedef struct {
	volatile int done;
	volatile int ok;
	ChiakiRegisteredHost host;
} RegCtx;

static void regist_cb(ChiakiRegistEvent *ev, void *user)
{
	RegCtx *c = user;
	if(ev->type == CHIAKI_REGIST_EVENT_TYPE_FINISHED_SUCCESS && ev->registered_host)
	{
		c->host = *ev->registered_host;
		c->ok = 1;
	}
	c->done = 1;
}

int ol_register(const char *host, const char *system_version, int ps5,
		const uint8_t account_id[8], uint32_t pin, OlRegistered *out,
		char *err, size_t err_size, int timeout_ms)
{
	ol_init();
	ChiakiLog log;
	make_log(&log);

	ChiakiDiscoveryHost dh;
	memset(&dh, 0, sizeof(dh));
	dh.system_version = system_version;
	dh.device_discovery_protocol_version = ps5 ? "00030010" : "00020020";
	ChiakiTarget target = chiaki_discovery_host_system_version_target(&dh);

	ChiakiRegistInfo info;
	memset(&info, 0, sizeof(info));
	info.target = target;
	info.host = host;
	info.broadcast = false;
	info.psn_online_id = NULL;
	memcpy(info.psn_account_id, account_id, CHIAKI_PSN_ACCOUNT_ID_SIZE);
	info.pin = pin;

	RegCtx ctx;
	memset(&ctx, 0, sizeof(ctx));
	ChiakiRegist regist;
	ChiakiErrorCode e = chiaki_regist_start(&regist, &log, &info, regist_cb, &ctx);
	if(e != CHIAKI_ERR_SUCCESS)
	{
		snprintf(err, err_size, "Registrierung konnte nicht gestartet werden: %s", chiaki_error_string(e));
		return 1;
	}
	int waited = 0;
	while(!ctx.done && waited < timeout_ms)
	{
		Sleep(100);
		waited += 100;
	}
	if(!ctx.done)
		chiaki_regist_stop(&regist);
	chiaki_regist_fini(&regist);

	if(!ctx.ok)
	{
		snprintf(err, err_size, ctx.done ? "Die Konsole hat die Kopplung abgelehnt (PIN oder Konto-ID falsch?)." : "Zeitueberschreitung bei der Kopplung.");
		return 2;
	}
	memset(out, 0, sizeof(*out));
	out->target = (int)ctx.host.target;
	out->ps5 = ps5;
	memcpy(out->nickname, ctx.host.server_nickname, sizeof(out->nickname));
	out->nickname[sizeof(out->nickname) - 1] = 0;
	memcpy(out->rp_regist_key, ctx.host.rp_regist_key, 16);
	memcpy(out->rp_key, ctx.host.rp_key, 16);
	out->rp_key_type = ctx.host.rp_key_type;
	return 0;
}

// ------------------------------------------------------------------ Stream
#define AUDIO_RING_SAMPLES (48000 * 2 * 2) // ca. 2 Sekunden Stereo

struct RpAu { uint8_t *d; size_t n; uint64_t t; int key; };
struct RpAud { int16_t *d; size_t frames; uint32_t ch; uint64_t t; };

struct OlStream {
	ChiakiLog log;
	ChiakiSession session;
	ChiakiFfmpegDecoder decoder;
	ChiakiOpusDecoder opus;
	ChiakiOpusEncoder mic_enc;
	int mic_ready, mic_connected, mic_live;
	int session_ok, decoder_ok, opus_ok;

	ChiakiMutex mtx;
	int state;
	int quit_error;
	char message[256];

	// Video (RGBA)
	uint8_t *rgba;
	size_t rgba_cap;
	int width, height;
	int frame_ready;
	uint64_t frames_decoded;
	int32_t frames_lost;
	struct SwsContext *sws;
	struct SwsContext *sws12;
	int sws12_w, sws12_h, sws12_fmt;
	uint8_t *nv12;
	size_t nv12_cap;
	int nv12_ready;
	int want_rgba;
	int sws_w, sws_h, sws_fmt;

	// Vibration
	uint8_t rumble_l, rumble_r;
	int rumble_new;

	// Controller (DualSense)
	ChiakiControllerState ctrl_state;
	ChiakiOrientationTracker orient;
	ChiakiAccelNewZero accel_zero;
	int touch_id[2];
	OlCtrlEvents ev;
	int ev_any;

	// Messwerte
	uint64_t t_sample_us;
	double decode_ms;
	uint64_t bytes_acc;
	uint64_t rate_t0_us;
	uint32_t bitrate_kbps;

	// Audio
	int16_t audio[AUDIO_RING_SAMPLES];
	size_t a_read, a_write; // monoton steigend
	uint32_t audio_rate, audio_channels;
	uint64_t audio_total;

	// Aufnahme (MP4, Video unveraendert uebernommen, Ton als AAC)
	ChiakiMutex rec_mtx;
	int codec_hevc;
	int rec_state; // 0 aus, 1 wartet auf Schluesselbild, 2 laeuft
	AVFormatContext *rec_fc;
	AVStream *rec_vs, *rec_as;
	AVCodecContext *rec_aenc;
	AVFrame *rec_aframe;
	int rec_acount;
	int64_t rec_apts;
	uint64_t rec_t0_us;
	int64_t rec_last_vpts;
	uint8_t *vhdr;      // Parameter-Sets (SPS/PPS bzw. VPS/SPS/PPS), die die Konsole vor den Bildern schickt
	size_t vhdr_len;
	uint64_t rec_bytes;
	char rec_path[600];
	int rec_use_ts;       // Zeitstempel von aussen (fuer Replay-Clips)
	uint64_t rec_ts_us;

	// Replay-Puffer: die letzten Sekunden Video und Ton im Speicher
	ChiakiMutex rp_mtx;
	int rp_secs;
	struct RpAu *rp_au;
	int rp_au_cap, rp_au_head, rp_au_cnt;
	struct RpAud *rp_aud;
	int rp_aud_cap, rp_aud_head, rp_aud_cnt;
	uint64_t rp_bytes;
};

// ------------------------------------------------------------------ Aufnahme
static int is_keyframe(const uint8_t *b, size_t n, int hevc)
{
	for(size_t i = 0; i + 3 < n; i++)
	{
		if(b[i] == 0 && b[i + 1] == 0 && b[i + 2] == 1)
		{
			uint8_t nal = b[i + 3];
			if(!hevc)
			{
				int t = nal & 0x1f;
				if(t == 5) return 1;
				if(t >= 1 && t <= 4) return 0;
			}
			else
			{
				int t = (nal >> 1) & 0x3f;
				if(t >= 16 && t <= 21) return 1;
				if(t < 16) return 0;
			}
			i += 3;
		}
	}
	return 0;
}

// 1, wenn der Puffer Parameter-Sets enthaelt (H.264: SPS 7 / PPS 8, HEVC: VPS 32 / SPS 33 / PPS 34)
static int has_param_sets(const uint8_t *b, size_t n, int hevc)
{
	for(size_t i = 0; i + 3 < n; i++)
	{
		if(b[i] == 0 && b[i + 1] == 0 && b[i + 2] == 1)
		{
			int t = hevc ? ((b[i + 3] >> 1) & 0x3f) : (b[i + 3] & 0x1f);
			if(hevc ? (t >= 32 && t <= 34) : (t == 7 || t == 8))
				return 1;
			i += 3;
		}
	}
	return 0;
}

static void rec_cleanup_locked(OlStream *s, int delete_file)
{
	if(s->rec_aenc) avcodec_free_context(&s->rec_aenc);
	if(s->rec_aframe) av_frame_free(&s->rec_aframe);
	if(s->rec_fc)
	{
		if(s->rec_fc->pb) avio_closep(&s->rec_fc->pb);
		avformat_free_context(s->rec_fc);
		s->rec_fc = NULL;
	}
	s->rec_vs = s->rec_as = NULL;
	s->rec_state = 0;
	s->rec_acount = 0;
	if(delete_file && s->rec_path[0]) remove(s->rec_path);
}

static void rec_write_audio_packets(OlStream *s, AVFrame *frame)
{
	if(avcodec_send_frame(s->rec_aenc, frame) < 0) return;
	AVPacket *pkt = av_packet_alloc();
	while(pkt && avcodec_receive_packet(s->rec_aenc, pkt) == 0)
	{
		pkt->stream_index = s->rec_as->index;
		av_packet_rescale_ts(pkt, s->rec_aenc->time_base, s->rec_as->time_base);
		s->rec_bytes += (uint64_t)pkt->size;
		av_interleaved_write_frame(s->rec_fc, pkt);
		av_packet_unref(pkt);
	}
	av_packet_free(&pkt);
}

static void rec_video(OlStream *s, const uint8_t *buf, size_t size)
{
	chiaki_mutex_lock(&s->rec_mtx);
	if(size > 0 && size < 4096)
	{
		// Die Konsole schickt SPS/PPS als eigenes kleines Paket vor den Bildern: merken (sonst fehlt der Aufnahme das Videoprofil)
		if(has_param_sets(buf, size, s->codec_hevc) && !is_keyframe(buf, size, s->codec_hevc))
		{
			uint8_t *c = (uint8_t *)malloc(size);
			if(c)
			{
				memcpy(c, buf, size);
				free(s->vhdr);
				s->vhdr = c;
				s->vhdr_len = size;
			}
		}
	}
	if(s->rec_state == 0 || size == 0)
	{
		chiaki_mutex_unlock(&s->rec_mtx);
		return;
	}
	int key = is_keyframe(buf, size, s->codec_hevc);
	if(s->rec_state == 1)
	{
		if(!key)
		{
			chiaki_mutex_unlock(&s->rec_mtx);
			return;
		}
		AVCodecParameters *par = s->rec_vs->codecpar;
		// Fehlen SPS/PPS im Schluesselbild, den gemerkten Header voranstellen
		size_t pre = (!has_param_sets(buf, size, s->codec_hevc) && s->vhdr && s->vhdr_len) ? s->vhdr_len : 0;
		par->extradata = av_mallocz(pre + size + AV_INPUT_BUFFER_PADDING_SIZE);
		if(!par->extradata)
		{
			rec_cleanup_locked(s, 1);
			chiaki_mutex_unlock(&s->rec_mtx);
			return;
		}
		if(pre) memcpy(par->extradata, s->vhdr, pre);
		memcpy(par->extradata + pre, buf, size);
		par->extradata_size = (int)(pre + size);
		AVDictionary *opts = NULL;
		av_dict_set(&opts, "movflags", "frag_keyframe+empty_moov+default_base_moof", 0);
		int r = avformat_write_header(s->rec_fc, &opts);
		av_dict_free(&opts);
		if(r < 0)
		{
			rec_cleanup_locked(s, 1);
			chiaki_mutex_unlock(&s->rec_mtx);
			return;
		}
		s->rec_t0_us = s->rec_use_ts ? s->rec_ts_us : now_us();
		s->rec_last_vpts = -1;
		s->rec_state = 2;
	}
	AVPacket *pkt = av_packet_alloc();
	if(pkt && av_new_packet(pkt, (int)size) == 0)
	{
		memcpy(pkt->data, buf, size);
		int64_t pts = av_rescale_q((int64_t)((s->rec_use_ts ? s->rec_ts_us : now_us()) - s->rec_t0_us), (AVRational){1, 1000000}, s->rec_vs->time_base);
		if(pts <= s->rec_last_vpts) pts = s->rec_last_vpts + 1;
		s->rec_last_vpts = pts;
		pkt->pts = pkt->dts = pts;
		pkt->stream_index = s->rec_vs->index;
		if(key) pkt->flags |= AV_PKT_FLAG_KEY;
		s->rec_bytes += (uint64_t)size;
		av_interleaved_write_frame(s->rec_fc, pkt);
	}
	av_packet_free(&pkt);
	chiaki_mutex_unlock(&s->rec_mtx);
}

static void rec_audio(OlStream *s, const int16_t *buf, size_t frames, uint32_t channels)
{
	chiaki_mutex_lock(&s->rec_mtx);
	if(s->rec_state != 2 || !s->rec_aenc || channels == 0)
	{
		chiaki_mutex_unlock(&s->rec_mtx);
		return;
	}
	AVFrame *f = s->rec_aframe;
	for(size_t i = 0; i < frames; i++)
	{
		if(s->rec_acount == 0 && av_frame_make_writable(f) < 0)
			break;
		float l = (float)buf[i * channels] / 32768.0f;
		float r = channels > 1 ? (float)buf[i * channels + 1] / 32768.0f : l;
		((float *)f->data[0])[s->rec_acount] = l;
		((float *)f->data[1])[s->rec_acount] = r;
		s->rec_acount++;
		if(s->rec_acount >= f->nb_samples)
		{
			f->pts = s->rec_apts;
			s->rec_apts += f->nb_samples;
			rec_write_audio_packets(s, f);
			s->rec_acount = 0;
		}
	}
	chiaki_mutex_unlock(&s->rec_mtx);
}

static int rec_open(OlStream *s, const char *path, char *err, size_t err_size)
{
	chiaki_mutex_lock(&s->rec_mtx);
	if(s->rec_state != 0)
	{
		chiaki_mutex_unlock(&s->rec_mtx);
		snprintf(err, err_size, "Eine Aufnahme laeuft schon.");
		return 1;
	}
	int w = s->width, h = s->height;
	if(w <= 0 || h <= 0)
	{
		chiaki_mutex_unlock(&s->rec_mtx);
		snprintf(err, err_size, "Noch kein Bild vorhanden.");
		return 2;
	}
	snprintf(s->rec_path, sizeof(s->rec_path), "%s", path);
	if(avformat_alloc_output_context2(&s->rec_fc, NULL, "mp4", path) < 0 || !s->rec_fc)
	{
		snprintf(err, err_size, "MP4 nicht moeglich.");
		goto fail;
	}
	s->rec_vs = avformat_new_stream(s->rec_fc, NULL);
	s->rec_as = avformat_new_stream(s->rec_fc, NULL);
	if(!s->rec_vs || !s->rec_as)
	{
		snprintf(err, err_size, "Stream nicht moeglich.");
		goto fail;
	}
	AVCodecParameters *vp = s->rec_vs->codecpar;
	vp->codec_type = AVMEDIA_TYPE_VIDEO;
	vp->codec_id = s->codec_hevc ? AV_CODEC_ID_HEVC : AV_CODEC_ID_H264;
	vp->codec_tag = s->codec_hevc ? MKTAG('h', 'v', 'c', '1') : 0;
	vp->width = w;
	vp->height = h;
	s->rec_vs->time_base = (AVRational){1, 90000};

	const AVCodec *ac = avcodec_find_encoder(AV_CODEC_ID_AAC);
	if(!ac)
	{
		snprintf(err, err_size, "AAC-Encoder fehlt.");
		goto fail;
	}
	s->rec_aenc = avcodec_alloc_context3(ac);
	if(!s->rec_aenc)
		goto fail;
	s->rec_aenc->sample_fmt = AV_SAMPLE_FMT_FLTP;
	s->rec_aenc->sample_rate = 48000;
	av_channel_layout_default(&s->rec_aenc->ch_layout, 2);
	s->rec_aenc->bit_rate = 192000;
	s->rec_aenc->time_base = (AVRational){1, 48000};
	if(s->rec_fc->oformat->flags & AVFMT_GLOBALHEADER)
		s->rec_aenc->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
	if(avcodec_open2(s->rec_aenc, ac, NULL) < 0)
	{
		snprintf(err, err_size, "AAC-Encoder startet nicht.");
		goto fail;
	}
	avcodec_parameters_from_context(s->rec_as->codecpar, s->rec_aenc);
	s->rec_as->time_base = (AVRational){1, 48000};
	s->rec_aframe = av_frame_alloc();
	if(!s->rec_aframe)
		goto fail;
	s->rec_aframe->nb_samples = s->rec_aenc->frame_size;
	s->rec_aframe->format = AV_SAMPLE_FMT_FLTP;
	s->rec_aframe->sample_rate = 48000;
	av_channel_layout_copy(&s->rec_aframe->ch_layout, &s->rec_aenc->ch_layout);
	if(av_frame_get_buffer(s->rec_aframe, 0) < 0)
		goto fail;
	if(avio_open(&s->rec_fc->pb, path, AVIO_FLAG_WRITE) < 0)
	{
		snprintf(err, err_size, "Datei kann nicht geschrieben werden.");
		goto fail;
	}
	s->rec_acount = 0;
	s->rec_apts = 0;
	s->rec_bytes = 0;
	s->rec_state = 1;
	chiaki_mutex_unlock(&s->rec_mtx);
	return 0;

fail:
	if(err[0] == 0)
		snprintf(err, err_size, "Aufnahme konnte nicht gestartet werden.");
	if(s->rec_fc && s->rec_fc->pb == NULL) { /* nichts geoeffnet */ }
	rec_cleanup_locked(s, 1);
	chiaki_mutex_unlock(&s->rec_mtx);
	return 3;
}

int ol_record_start(OlStream *s, const char *path, char *err, size_t err_size)
{
	int r = rec_open(s, path, err, err_size);
	// sofort ein Schluesselbild anfordern, damit die Aufnahme ohne Wartezeit beginnt
	if(r == 0)
		chiaki_session_request_idr(&s->session);
	return r;
}

void ol_record_stop(OlStream *s)
{
	chiaki_mutex_lock(&s->rec_mtx);
	if(s->rec_state == 2)
	{
		// Rest des Tons ausschreiben und Datei abschliessen
		if(s->rec_acount > 0)
		{
			s->rec_aframe->nb_samples = s->rec_acount;
			s->rec_aframe->pts = s->rec_apts;
			rec_write_audio_packets(s, s->rec_aframe);
		}
		rec_write_audio_packets(s, NULL);
		av_write_trailer(s->rec_fc);
		rec_cleanup_locked(s, 0);
	}
	else if(s->rec_state == 1)
	{
		rec_cleanup_locked(s, 1);
	}
	chiaki_mutex_unlock(&s->rec_mtx);
}

// 0 aus, 1 wartet auf Schluesselbild, 2 laeuft
int ol_record_state(OlStream *s)
{
	chiaki_mutex_lock(&s->rec_mtx);
	int r = s->rec_state;
	chiaki_mutex_unlock(&s->rec_mtx);
	return r;
}

uint64_t ol_record_bytes(OlStream *s)
{
	chiaki_mutex_lock(&s->rec_mtx);
	uint64_t r = s->rec_bytes;
	chiaki_mutex_unlock(&s->rec_mtx);
	return r;
}

// ------------------------------------------------------------------ Replay-Puffer
static void rp_free_au(struct RpAu *a) { free(a->d); a->d = NULL; }
static void rp_free_aud(struct RpAud *a) { free(a->d); a->d = NULL; }

static void rp_trim_locked(OlStream *s, uint64_t now)
{
	uint64_t keep = (uint64_t)(s->rp_secs + 8) * 1000000ULL;
	while(s->rp_au_cnt > 0)
	{
		struct RpAu *a = &s->rp_au[s->rp_au_head];
		if(now - a->t <= keep && s->rp_bytes < 300ULL * 1024 * 1024)
			break;
		s->rp_bytes -= a->n;
		rp_free_au(a);
		s->rp_au_head = (s->rp_au_head + 1) % s->rp_au_cap;
		s->rp_au_cnt--;
	}
	while(s->rp_aud_cnt > 0)
	{
		struct RpAud *a = &s->rp_aud[s->rp_aud_head];
		if(now - a->t <= keep)
			break;
		rp_free_aud(a);
		s->rp_aud_head = (s->rp_aud_head + 1) % s->rp_aud_cap;
		s->rp_aud_cnt--;
	}
}

static void rp_push_video(OlStream *s, const uint8_t *buf, size_t size)
{
	if(s->rp_secs <= 0 || size < 64)   // kleine Pakete sind Parameter-Sets, die merkt sich rec_video
		return;
	chiaki_mutex_lock(&s->rp_mtx);
	if(s->rp_secs > 0 && s->rp_au)
	{
		uint64_t now = now_us();
		rp_trim_locked(s, now);
		if(s->rp_au_cnt >= s->rp_au_cap)
		{
			struct RpAu *o = &s->rp_au[s->rp_au_head];
			s->rp_bytes -= o->n;
			rp_free_au(o);
			s->rp_au_head = (s->rp_au_head + 1) % s->rp_au_cap;
			s->rp_au_cnt--;
		}
		uint8_t *c = (uint8_t *)malloc(size);
		if(c)
		{
			memcpy(c, buf, size);
			struct RpAu *a = &s->rp_au[(s->rp_au_head + s->rp_au_cnt) % s->rp_au_cap];
			a->d = c; a->n = size; a->t = now; a->key = is_keyframe(buf, size, s->codec_hevc);
			s->rp_au_cnt++;
			s->rp_bytes += size;
		}
	}
	chiaki_mutex_unlock(&s->rp_mtx);
}

static void rp_push_audio(OlStream *s, const int16_t *buf, size_t frames, uint32_t ch)
{
	if(s->rp_secs <= 0 || frames == 0 || ch == 0)
		return;
	chiaki_mutex_lock(&s->rp_mtx);
	if(s->rp_secs > 0 && s->rp_aud)
	{
		if(s->rp_aud_cnt >= s->rp_aud_cap)
		{
			struct RpAud *o = &s->rp_aud[s->rp_aud_head];
			rp_free_aud(o);
			s->rp_aud_head = (s->rp_aud_head + 1) % s->rp_aud_cap;
			s->rp_aud_cnt--;
		}
		size_t bytes = frames * ch * sizeof(int16_t);
		int16_t *c = (int16_t *)malloc(bytes);
		if(c)
		{
			memcpy(c, buf, bytes);
			struct RpAud *a = &s->rp_aud[(s->rp_aud_head + s->rp_aud_cnt) % s->rp_aud_cap];
			a->d = c; a->frames = frames; a->ch = ch; a->t = now_us();
			s->rp_aud_cnt++;
		}
	}
	chiaki_mutex_unlock(&s->rp_mtx);
}

static void rp_clear_locked(OlStream *s)
{
	for(int i = 0; i < s->rp_au_cnt; i++)
		rp_free_au(&s->rp_au[(s->rp_au_head + i) % s->rp_au_cap]);
	for(int i = 0; i < s->rp_aud_cnt; i++)
		rp_free_aud(&s->rp_aud[(s->rp_aud_head + i) % s->rp_aud_cap]);
	free(s->rp_au); s->rp_au = NULL;
	free(s->rp_aud); s->rp_aud = NULL;
	s->rp_au_cnt = s->rp_au_head = s->rp_aud_cnt = s->rp_aud_head = 0;
	s->rp_bytes = 0;
}

static bool video_sample(uint8_t *buf, size_t buf_size, int32_t frames_lost, bool frame_recovered, void *user)
{
	OlStream *s = user;
	s->t_sample_us = now_us();
	s->bytes_acc += buf_size;
	rec_video(s, buf, buf_size);
	rp_push_video(s, buf, buf_size);
	return chiaki_ffmpeg_decoder_video_sample_cb(buf, buf_size, frames_lost, frame_recovered, &s->decoder);
}

static void frame_available(ChiakiFfmpegDecoder *decoder, void *user)
{
	OlStream *s = user;
	if(s->t_sample_us)
	{
		double ms = (double)(now_us() - s->t_sample_us) / 1000.0;
		if(ms >= 0.0 && ms < 500.0)
			s->decode_ms = s->decode_ms * 0.9 + ms * 0.1;
	}
	int32_t lost = 0;
	ChiakiFfmpegFrame f = chiaki_ffmpeg_decoder_pull_frame(decoder, &lost);
	if(!f.frame)
		return;
	AVFrame *frame = f.frame;
	AVFrame *sw = NULL;
	if(frame->format == decoder->hw_pix_fmt && decoder->hw_pix_fmt != AV_PIX_FMT_NONE)
	{
		sw = av_frame_alloc();
		if(!sw || av_hwframe_transfer_data(sw, frame, 0) < 0)
		{
			av_frame_free(&sw);
			av_frame_free(&frame);
			return;
		}
		frame = sw;
	}

	int w = frame->width, h = frame->height;
	chiaki_mutex_lock(&s->mtx);

	// --- NV12 direkt fuer die Grafikkarte (kein CPU-Umrechnen) ---
	{
		size_t cw = ((size_t)w + 1) / 2, chh = ((size_t)h + 1) / 2;
		size_t ysz = (size_t)w * h;
		size_t need12 = ysz + cw * 2 * chh;
		if(s->nv12_cap < need12)
		{
			free(s->nv12);
			s->nv12 = malloc(need12);
			s->nv12_cap = s->nv12 ? need12 : 0;
		}
		if(s->nv12)
		{
			int done = 0;
			if(frame->format == AV_PIX_FMT_NV12)
			{
				for(int y = 0; y < h; y++)
					memcpy(s->nv12 + (size_t)y * w, frame->data[0] + (size_t)y * frame->linesize[0], (size_t)w);
				for(size_t y = 0; y < chh; y++)
					memcpy(s->nv12 + ysz + y * cw * 2, frame->data[1] + y * frame->linesize[1], cw * 2);
				done = 1;
			}
			else if(frame->format == AV_PIX_FMT_YUV420P || frame->format == AV_PIX_FMT_YUVJ420P)
			{
				for(int y = 0; y < h; y++)
					memcpy(s->nv12 + (size_t)y * w, frame->data[0] + (size_t)y * frame->linesize[0], (size_t)w);
				for(size_t y = 0; y < chh; y++)
				{
					const uint8_t *u = frame->data[1] + y * frame->linesize[1];
					const uint8_t *v = frame->data[2] + y * frame->linesize[2];
					uint8_t *dst = s->nv12 + ysz + y * cw * 2;
					for(size_t x = 0; x < cw; x++)
					{
						dst[x * 2] = u[x];
						dst[x * 2 + 1] = v[x];
					}
				}
				done = 1;
			}
			else
			{
				// anderes Format (z. B. 10 Bit): ueber swscale nach NV12
				if(!s->sws12 || s->sws12_w != w || s->sws12_h != h || s->sws12_fmt != frame->format)
				{
					sws_freeContext(s->sws12);
					s->sws12 = sws_getContext(w, h, frame->format, w, h, AV_PIX_FMT_NV12, SWS_FAST_BILINEAR, NULL, NULL, NULL);
					s->sws12_w = w; s->sws12_h = h; s->sws12_fmt = frame->format;
				}
				if(s->sws12)
				{
					uint8_t *dst[4] = { s->nv12, s->nv12 + ysz, NULL, NULL };
					int dst_stride[4] = { w, (int)(cw * 2), 0, 0 };
					sws_scale(s->sws12, (const uint8_t *const *)frame->data, frame->linesize, 0, h, dst, dst_stride);
					done = 1;
				}
			}
			if(done)
			{
				s->width = w;
				s->height = h;
				s->nv12_ready = 1;
				s->frames_decoded++;
			}
		}
	}

	// --- RGBA (alter Weg, nur auf Wunsch) ---
	if(s->want_rgba)
	{
		size_t need = (size_t)w * h * 4;
		if(s->rgba_cap < need)
		{
			free(s->rgba);
			s->rgba = malloc(need);
			s->rgba_cap = s->rgba ? need : 0;
		}
		if(s->rgba)
		{
			if(!s->sws || s->sws_w != w || s->sws_h != h || s->sws_fmt != frame->format)
			{
				sws_freeContext(s->sws);
				s->sws = sws_getContext(w, h, frame->format, w, h, AV_PIX_FMT_RGBA, SWS_FAST_BILINEAR, NULL, NULL, NULL);
				s->sws_w = w; s->sws_h = h; s->sws_fmt = frame->format;
			}
			if(s->sws)
			{
				uint8_t *dst[4] = { s->rgba, NULL, NULL, NULL };
				int dst_stride[4] = { w * 4, 0, 0, 0 };
				sws_scale(s->sws, (const uint8_t *const *)frame->data, frame->linesize, 0, h, dst, dst_stride);
				s->width = w;
				s->height = h;
				s->frame_ready = 1;
			}
		}
	}
	s->frames_lost += lost;
	chiaki_mutex_unlock(&s->mtx);

	av_frame_free(&sw);
	av_frame_free(&f.frame);
}

static void opus_settings_cb(uint32_t channels, uint32_t rate, void *user)
{
	OlStream *s = user;
	chiaki_mutex_lock(&s->mtx);
	s->audio_channels = channels;
	s->audio_rate = rate;
	chiaki_mutex_unlock(&s->mtx);
}

static void opus_frame_cb(int16_t *buf, size_t samples_count, void *user)
{
	OlStream *s = user;
	size_t total = samples_count * (s->audio_channels ? s->audio_channels : 2);
	rec_audio(s, buf, samples_count, s->audio_channels ? s->audio_channels : 2);
	rp_push_audio(s, buf, samples_count, s->audio_channels ? s->audio_channels : 2);
	chiaki_mutex_lock(&s->mtx);
	s->audio_total += samples_count;
	for(size_t i = 0; i < total; i++)
	{
		s->audio[s->a_write % AUDIO_RING_SAMPLES] = buf[i];
		s->a_write++;
	}
	if(s->a_write - s->a_read > AUDIO_RING_SAMPLES)
		s->a_read = s->a_write - AUDIO_RING_SAMPLES;
	chiaki_mutex_unlock(&s->mtx);
}

static void event_cb(ChiakiEvent *ev, void *user)
{
	OlStream *s = user;
	chiaki_mutex_lock(&s->mtx);
	switch(ev->type)
	{
		case CHIAKI_EVENT_CONNECTED:
			s->state = 1;
			snprintf(s->message, sizeof(s->message), "Verbunden");
			break;
		case CHIAKI_EVENT_QUIT:
			s->state = 2;
			s->quit_error = chiaki_quit_reason_is_error(ev->quit.reason);
			snprintf(s->message, sizeof(s->message), "%s%s%s", chiaki_quit_reason_string(ev->quit.reason),
				ev->quit.reason_str ? ": " : "", ev->quit.reason_str ? ev->quit.reason_str : "");
			break;
		case CHIAKI_EVENT_TRIGGER_EFFECTS:
			s->ev.trigger_new = 1;
			s->ev.type_left = ev->trigger_effects.type_left;
			s->ev.type_right = ev->trigger_effects.type_right;
			memcpy(s->ev.left, ev->trigger_effects.left, 10);
			memcpy(s->ev.right, ev->trigger_effects.right, 10);
			s->ev_any = 1;
			break;
		case CHIAKI_EVENT_LED_COLOR:
			s->ev.led_new = 1;
			memcpy(s->ev.led, ev->led_state, 3);
			s->ev_any = 1;
			break;
		case CHIAKI_EVENT_PLAYER_INDEX:
			s->ev.player_new = 1;
			s->ev.player = ev->player_index;
			s->ev_any = 1;
			break;
		case CHIAKI_EVENT_RUMBLE:
			s->rumble_l = ev->rumble.left;
			s->rumble_r = ev->rumble.right;
			s->rumble_new = 1;
			break;
		case CHIAKI_EVENT_LOGIN_PIN_REQUEST:
			snprintf(s->message, sizeof(s->message), "Die Konsole verlangt eine Login-PIN");
			break;
		default:
			break;
	}
	chiaki_mutex_unlock(&s->mtx);
}

int ol_psn_duid(char *out, size_t out_size)
{
	ol_init();
	if(out_size < CHIAKI_DUID_STR_SIZE)
		return 1;
	size_t n = out_size;
	return chiaki_holepunch_generate_client_device_uid(out, &n) == CHIAKI_ERR_SUCCESS ? 0 : 1;
}

int ol_psn_list(const char *token, OlPsnDevice *out, int max, char *err, size_t err_size)
{
	ol_init();
	static ChiakiLog log;
	make_log(&log);
	ChiakiHolepunchDeviceInfo *dev = NULL;
	size_t count = 0;
	ChiakiErrorCode e = chiaki_holepunch_list_devices(token, CHIAKI_HOLEPUNCH_CONSOLE_TYPE_PS5, &dev, &count, &log);
	if(e != CHIAKI_ERR_SUCCESS)
	{
		snprintf(err, err_size, "PSN-Konsolenliste nicht erreichbar: %s", chiaki_error_string(e));
		return -1;
	}
	int n = 0;
	for(size_t i = 0; i < count && n < max; i++)
	{
		if(!dev[i].remoteplay_enabled)
			continue;
		memset(&out[n], 0, sizeof(out[n]));
		out[n].ps5 = 1;
		memcpy(out[n].name, dev[i].device_name, sizeof(out[n].name) - 1);
		memcpy(out[n].uid, dev[i].device_uid, 32);
		n++;
	}
	if(dev)
		chiaki_holepunch_free_device_list(&dev);
	return n;
}

// Pruefung ohne Konto: oeffnet nur den Benachrichtigungs-Kanal mit absichtlich ungueltiger Anmeldung.
void ol_psn_wstest(const char *host)
{
	ol_init();
	static ChiakiLog log;
	make_log(&log);
	ChiakiHolepunchSession hp = chiaki_holepunch_session_init("ungueltig", &log);
	if(!hp)
		return;
	chiaki_holepunch_session_set_ws_fqdn_override(hp, host);
	chiaki_holepunch_session_create(hp);
	chiaki_holepunch_session_fini(hp);
}

// Baut die Verbindung ueber PSN auf (Steuer-Kanal). Der Daten-Kanal folgt in der Sitzung selbst.
static ChiakiHolepunchSession psn_connect(OlStream *s, const OlStreamParams *p, char *err, size_t err_size)
{
	ChiakiHolepunchSession hp = chiaki_holepunch_session_init(p->psn_token, &s->log);
	if(!hp)
	{
		snprintf(err, err_size, "PSN-Verbindung konnte nicht vorbereitet werden");
		return NULL;
	}
	// Ohne UPnP im Router geht es trotzdem weiter
	chiaki_holepunch_upnp_discover(hp);
	const char *step = "Anmeldung bei PSN (eventuell unter Von unterwegs neu anmelden)";
	ChiakiErrorCode e = chiaki_holepunch_session_create(hp);
	if(e == CHIAKI_ERR_SUCCESS)
	{
		step = "Verbindungsangebot";
		e = holepunch_session_create_offer(hp);
	}
	if(e == CHIAKI_ERR_SUCCESS)
	{
		step = "Konsole erreichen (ist sie an oder im Ruhemodus und mit dem Internet verbunden?)";
		e = chiaki_holepunch_session_start(hp, p->psn_uid, p->ps5 ? CHIAKI_HOLEPUNCH_CONSOLE_TYPE_PS5 : CHIAKI_HOLEPUNCH_CONSOLE_TYPE_PS4);
	}
	if(e == CHIAKI_ERR_SUCCESS)
	{
		step = "Direkte Verbindung durch die Router (bei Mobilfunk oder DS-Lite oft nicht moeglich, dann VPN nutzen)";
		e = chiaki_holepunch_session_punch_hole(hp, CHIAKI_HOLEPUNCH_PORT_TYPE_CTRL);
	}
	if(e != CHIAKI_ERR_SUCCESS)
	{
		snprintf(err, err_size, "Fehlgeschlagen bei: %s. (%s)", step, chiaki_error_string(e));
		chiaki_holepunch_session_fini(hp);
		return NULL;
	}
	return hp;
}

OlStream *ol_stream_start(const OlStreamParams *p, char *err, size_t err_size)
{
	ol_init();
	OlStream *s = calloc(1, sizeof(*s));
	if(!s)
		return NULL;
	make_log(&s->log);
	chiaki_mutex_init(&s->mtx, false);
	chiaki_mutex_init(&s->rec_mtx, false);
	chiaki_mutex_init(&s->rp_mtx, false);
	s->audio_channels = 2;
	s->audio_rate = 48000;
	snprintf(s->message, sizeof(s->message), "Verbinde ...");

	ChiakiConnectInfo ci;
	memset(&ci, 0, sizeof(ci));
	ci.ps5 = p->ps5 ? true : false;
	ci.host = p->host;
	memcpy(ci.regist_key, p->rp_regist_key, 16);
	memcpy(ci.morning, p->rp_key, 16);
	ci.video_profile.width = (unsigned)p->width;
	ci.video_profile.height = (unsigned)p->height;
	ci.video_profile.max_fps = (unsigned)p->fps;
	ci.video_profile.bitrate = (unsigned)(p->bitrate_kbps > 0 ? p->bitrate_kbps : 15000);
	ci.video_profile.codec = p->codec ? CHIAKI_CODEC_H265 : CHIAKI_CODEC_H264;
	s->codec_hevc = p->codec ? 1 : 0;
	ci.video_profile_auto_downgrade = true;
	ci.enable_keyboard = false;
	ci.enable_dualsense = p->enable_dualsense ? true : false;
	ci.packet_loss_max = 0.05;
	ci.enable_idr_on_fec_failure = true;

	if(p->psn_token && p->psn_token[0])
	{
		ci.holepunch_session = psn_connect(s, p, err, err_size);
		if(!ci.holepunch_session)
			goto fail;
		memcpy(ci.psn_account_id, p->psn_account_id, 8);
	}

	ChiakiErrorCode e = chiaki_session_init(&s->session, &ci, &s->log);
	if(e != CHIAKI_ERR_SUCCESS)
	{
		snprintf(err, err_size, "Sitzung konnte nicht erstellt werden: %s", chiaki_error_string(e));
		goto fail;
	}
	s->session_ok = 1;
	chiaki_controller_state_set_idle(&s->ctrl_state);
	chiaki_orientation_tracker_init(&s->orient);
	chiaki_accel_new_zero_set_inactive(&s->accel_zero, false);
	s->touch_id[0] = s->touch_id[1] = -1;

	e = chiaki_ffmpeg_decoder_init(&s->decoder, &s->log, ci.video_profile.codec, (unsigned)p->fps,
		p->hw_decode ? "d3d11va" : NULL, NULL, frame_available, s);
	if(e != CHIAKI_ERR_SUCCESS && p->hw_decode)
	{
		// Fallback: Software-Decoder
		e = chiaki_ffmpeg_decoder_init(&s->decoder, &s->log, ci.video_profile.codec, (unsigned)p->fps, NULL, NULL, frame_available, s);
	}
	if(e != CHIAKI_ERR_SUCCESS)
	{
		snprintf(err, err_size, "Video-Decoder konnte nicht gestartet werden");
		goto fail;
	}
	s->decoder_ok = 1;

	chiaki_opus_decoder_init(&s->opus, &s->log);
	s->opus.settings_cb = opus_settings_cb;
	s->opus.frame_cb = opus_frame_cb;
	s->opus.cb_user = s;
	s->opus_ok = 1;
	ChiakiAudioSink sink;
	chiaki_opus_decoder_get_sink(&s->opus, &sink);
	chiaki_session_set_audio_sink(&s->session, &sink);
	// Mikrofon: Opus-Kodierer und Audio-Sender vorbereiten (Ton geht erst nach ol_mic_enable zur Konsole)
	{
		chiaki_opus_encoder_init(&s->mic_enc, &s->log);
		ChiakiAudioHeader mh;
		chiaki_audio_header_set(&mh, 2, 16, 48000, 480);
		chiaki_opus_encoder_header(&mh, &s->mic_enc, &s->session);
		s->mic_ready = s->mic_enc.audio_sender != NULL;
	}
	chiaki_session_set_video_sample_cb(&s->session, video_sample, s);
	chiaki_session_set_event_cb(&s->session, event_cb, s);

	e = chiaki_session_start(&s->session);
	if(e != CHIAKI_ERR_SUCCESS)
	{
		snprintf(err, err_size, "Verbindung konnte nicht gestartet werden: %s", chiaki_error_string(e));
		goto fail;
	}
	return s;

fail:
	if(s->opus_ok) chiaki_opus_decoder_fini(&s->opus);
	if(s->decoder_ok) chiaki_ffmpeg_decoder_fini(&s->decoder);
	if(s->session_ok) chiaki_session_fini(&s->session);
	chiaki_mutex_fini(&s->rec_mtx);
	chiaki_mutex_fini(&s->mtx);
	free(s);
	return NULL;
}

void ol_stream_status(OlStream *s, OlStatus *out)
{
	chiaki_mutex_lock(&s->mtx);
	out->state = s->state;
	out->quit_error = s->quit_error;
	snprintf(out->message, sizeof(out->message), "%s", s->message);
	out->frames_decoded = s->frames_decoded;
	out->width = s->width;
	out->height = s->height;
	out->audio_rate = s->audio_rate;
	out->audio_channels = s->audio_channels;
	out->frames_lost = s->frames_lost;
	out->audio_total = s->audio_total;
	out->rtt_us = s->session.rtt_us;
	out->decode_ms = (float)s->decode_ms;
	{
		uint64_t now = now_us();
		if(s->rate_t0_us == 0)
			s->rate_t0_us = now;
		if(now - s->rate_t0_us >= 1000000)
		{
			s->bitrate_kbps = (uint32_t)((double)s->bytes_acc * 8.0 / 1000.0 * 1000000.0 / (double)(now - s->rate_t0_us));
			s->bytes_acc = 0;
			s->rate_t0_us = now;
		}
		out->bitrate_kbps = s->bitrate_kbps;
	}
	chiaki_mutex_unlock(&s->mtx);
}

int ol_stream_take_frame(OlStream *s, uint8_t *buf, size_t buf_size, int *width, int *height)
{
	int got = 0;
	chiaki_mutex_lock(&s->mtx);
	if(s->frame_ready && s->rgba)
	{
		size_t need = (size_t)s->width * s->height * 4;
		if(need <= buf_size)
		{
			memcpy(buf, s->rgba, need);
			*width = s->width;
			*height = s->height;
			s->frame_ready = 0;
			got = 1;
		}
	}
	chiaki_mutex_unlock(&s->mtx);
	return got;
}

int ol_stream_take_nv12(OlStream *s, uint8_t *buf, size_t buf_size, int *width, int *height)
{
	int got = 0;
	chiaki_mutex_lock(&s->mtx);
	if(s->nv12_ready && s->nv12)
	{
		size_t cw = ((size_t)s->width + 1) / 2, chh = ((size_t)s->height + 1) / 2;
		size_t need = (size_t)s->width * s->height + cw * 2 * chh;
		if(need <= buf_size)
		{
			memcpy(buf, s->nv12, need);
			*width = s->width;
			*height = s->height;
			s->nv12_ready = 0;
			got = 1;
		}
	}
	chiaki_mutex_unlock(&s->mtx);
	return got;
}

void ol_stream_set_rgba(OlStream *s, int on)
{
	chiaki_mutex_lock(&s->mtx);
	s->want_rgba = on;
	chiaki_mutex_unlock(&s->mtx);
}

size_t ol_stream_take_audio(OlStream *s, int16_t *buf, size_t max_samples)
{
	chiaki_mutex_lock(&s->mtx);
	size_t avail = s->a_write - s->a_read;
	size_t n = avail < max_samples ? avail : max_samples;
	for(size_t i = 0; i < n; i++)
		buf[i] = s->audio[(s->a_read + i) % AUDIO_RING_SAMPLES];
	s->a_read += n;
	chiaki_mutex_unlock(&s->mtx);
	return n;
}

void ol_stream_set_input(OlStream *s, uint32_t buttons, uint8_t l2, uint8_t r2,
		int16_t lx, int16_t ly, int16_t rx, int16_t ry)
{
	ChiakiControllerState st;
	chiaki_controller_state_set_idle(&st);
	st.buttons = buttons;
	st.l2_state = l2;
	st.r2_state = r2;
	st.left_x = lx;
	st.left_y = ly;
	st.right_x = rx;
	st.right_y = ry;
	chiaki_session_set_controller_state(&s->session, &st);
}

void ol_stream_set_input_ex(OlStream *s, const OlInputEx *in)
{
	ChiakiControllerState *st = &s->ctrl_state;
	st->buttons = in->buttons;
	st->l2_state = in->l2;
	st->r2_state = in->r2;
	st->left_x = in->lx;
	st->left_y = in->ly;
	st->right_x = in->rx;
	st->right_y = in->ry;
	for(int i = 0; i < 2; i++)
	{
		if(in->touch_down[i])
		{
			uint16_t x = (uint16_t)(in->touch_x[i] * 1919.0f);
			uint16_t y = (uint16_t)(in->touch_y[i] * 1079.0f);
			if(s->touch_id[i] < 0)
			{
				int8_t id = chiaki_controller_state_start_touch(st, x, y);
				s->touch_id[i] = id;
			}
			else
				chiaki_controller_state_set_touch_pos(st, (uint8_t)s->touch_id[i], x, y);
		}
		else if(s->touch_id[i] >= 0)
		{
			chiaki_controller_state_stop_touch(st, (uint8_t)s->touch_id[i]);
			s->touch_id[i] = -1;
		}
	}
	if(in->has_motion)
	{
		chiaki_orientation_tracker_update(&s->orient, in->gyro[0], in->gyro[1], in->gyro[2],
			in->accel[0], in->accel[1], in->accel[2], &s->accel_zero, false, in->ts_us);
		chiaki_orientation_tracker_apply_to_controller_state(&s->orient, st);
	}
	chiaki_session_set_controller_state(&s->session, st);
}

int ol_stream_take_ctrl_events(OlStream *s, OlCtrlEvents *out)
{
	int any = 0;
	chiaki_mutex_lock(&s->mtx);
	if(s->ev_any)
	{
		*out = s->ev;
		memset(&s->ev, 0, sizeof(s->ev));
		s->ev_any = 0;
		any = 1;
	}
	chiaki_mutex_unlock(&s->mtx);
	return any;
}

int ol_stream_take_rumble(OlStream *s, uint8_t *left, uint8_t *right)
{
	int r = 0;
	chiaki_mutex_lock(&s->mtx);
	if(s->rumble_new)
	{
		*left = s->rumble_l;
		*right = s->rumble_r;
		s->rumble_new = 0;
		r = 1;
	}
	chiaki_mutex_unlock(&s->mtx);
	return r;
}

void ol_stream_stop(OlStream *s)
{
	/* vhdr wird unten mit s freigegeben */
	if(!s)
		return;
	chiaki_session_stop(&s->session);
	chiaki_session_join(&s->session);
	ol_record_stop(s);
	s->mic_live = 0;
	if(s->mic_ready) chiaki_opus_encoder_fini(&s->mic_enc);
	chiaki_opus_decoder_fini(&s->opus);
	chiaki_ffmpeg_decoder_fini(&s->decoder);
	chiaki_session_fini(&s->session);
	sws_freeContext(s->sws);
	sws_freeContext(s->sws12);
	free(s->nv12);
	free(s->rgba);
	chiaki_mutex_fini(&s->rec_mtx);
	chiaki_mutex_fini(&s->mtx);
	chiaki_mutex_lock(&s->rp_mtx);
	s->rp_secs = 0;
	rp_clear_locked(s);
	chiaki_mutex_unlock(&s->rp_mtx);
	free(s->vhdr);
	free(s);
}


// Selbsttest der Aufnahme ohne Konsole (nur fuer Entwickler).
int ol_record_selftest(const char *in_path, const char *out_path, int hevc, int w, int h, char *err, size_t err_size)
{
	FILE *f = fopen(in_path, "rb");
	if(!f)
	{
		snprintf(err, err_size, "Eingabe fehlt");
		return 10;
	}
	fseek(f, 0, SEEK_END);
	long n = ftell(f);
	fseek(f, 0, SEEK_SET);
	uint8_t *buf = malloc((size_t)n);
	if(!buf || fread(buf, 1, (size_t)n, f) != (size_t)n)
	{
		fclose(f);
		return 11;
	}
	fclose(f);
	OlStream *s = calloc(1, sizeof(*s));
	chiaki_mutex_init(&s->mtx, false);
	chiaki_mutex_init(&s->rec_mtx, false);
	chiaki_mutex_init(&s->rp_mtx, false);
	s->codec_hevc = hevc;
	s->width = w;
	s->height = h;
	err[0] = 0;
	int r = rec_open(s, out_path, err, err_size);
	if(r != 0)
	{
		free(buf);
		return r;
	}
	// Zugriffseinheiten an den AUD-Nalus trennen (H.264: Typ 9, HEVC: Typ 35)
	long starts[100000];
	int cnt = 0;
	for(long i = 0; i + 4 < n && cnt < 99999; i++)
	{
		if(buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1)
		{
			int t = hevc ? ((buf[i + 3] >> 1) & 0x3f) : (buf[i + 3] & 0x1f);
			if(t == (hevc ? 35 : 9))
			{
				long st = (i > 0 && buf[i - 1] == 0) ? i - 1 : i;
				starts[cnt++] = st;
			}
		}
	}
	starts[cnt] = n;
	int16_t tone[2 * 800];
	double ph = 0;
	for(int k = 0; k < cnt; k++)
	{
		{
			// Wie bei der Konsole: Parameter-Sets als eigenes Paket vor dem ersten Schluesselbild schicken
			const uint8_t *au = buf + starts[k];
			size_t aun = (size_t)(starts[k + 1] - starts[k]);
			size_t sp = 0;
			if(k == 0)
			{
				for(size_t q = 0; q + 3 < aun; q++)
				{
					if(au[q] == 0 && au[q + 1] == 0 && au[q + 2] == 1)
					{
						int t = hevc ? ((au[q + 3] >> 1) & 0x3f) : (au[q + 3] & 0x1f);
						if(hevc ? (t >= 16 && t <= 21) : (t == 5))
						{
							sp = (q > 0 && au[q - 1] == 0) ? q - 1 : q;
							break;
						}
					}
				}
			}
			if(sp > 0)
			{
				rec_video(s, au, sp);
				rec_video(s, au + sp, aun - sp);
			}
			else
				rec_video(s, au, aun);
		}
		for(int i = 0; i < 800; i++)
		{
			int16_t v = (int16_t)(sin(ph) * 8000);
			ph += 2.0 * 3.14159265358979 * 440.0 / 48000.0;
			tone[i * 2] = v;
			tone[i * 2 + 1] = v;
		}
		rec_audio(s, tone, 800, 2);
		Sleep(16);
	}
	ol_record_stop(s);
	free(buf);
	chiaki_mutex_fini(&s->rec_mtx);
	chiaki_mutex_fini(&s->mtx);
	free(s);
	return 0;
}

// ---------------------------------------------------------------- Video-Wiedergabe (Galerie)
#include <libswresample/swresample.h>

struct OlPlayer
{
	AVFormatContext *fc;
	int vi, ai;
	AVCodecContext *vdec, *adec;
	struct SwsContext *sws;
	struct SwrContext *swr;
	AVPacket *pkt;
	AVFrame *frame;
	int out_w, out_h;
	int src_w, src_h, src_fmt;
	int64_t duration_ms;
	int draining;
	int has_audio;
};

static int play_open_decoder(AVFormatContext *fc, int idx, AVCodecContext **out)
{
	AVStream *st = fc->streams[idx];
	const AVCodec *c = avcodec_find_decoder(st->codecpar->codec_id);
	if(!c) return -1;
	AVCodecContext *ctx = avcodec_alloc_context3(c);
	if(!ctx) return -1;
	if(avcodec_parameters_to_context(ctx, st->codecpar) < 0) { avcodec_free_context(&ctx); return -1; }
	ctx->thread_count = 0;
	if(avcodec_open2(ctx, c, NULL) < 0) { avcodec_free_context(&ctx); return -1; }
	*out = ctx;
	return 0;
}

OlPlayer *ol_play_open(const char *path, int max_w, int want, char *err, size_t err_size)
{
	OlPlayer *p = (OlPlayer *)calloc(1, sizeof(*p));
	if(!p) return NULL;
	p->vi = p->ai = -1;
	if(avformat_open_input(&p->fc, path, NULL, NULL) < 0)
	{
		snprintf(err, err_size, "Datei kann nicht geöffnet werden.");
		goto fail;
	}
	if(avformat_find_stream_info(p->fc, NULL) < 0)
	{
		snprintf(err, err_size, "Die Aufnahme ist beschädigt (Videoinformationen fehlen).");
		goto fail;
	}
	p->vi = av_find_best_stream(p->fc, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
	p->ai = av_find_best_stream(p->fc, AVMEDIA_TYPE_AUDIO, -1, -1, NULL, 0);
	if(p->vi < 0)
	{
		snprintf(err, err_size, "Kein Videobild in der Datei.");
		goto fail;
	}
	p->has_audio = p->ai >= 0;
	if(want & 1)
	{
		if(play_open_decoder(p->fc, p->vi, &p->vdec) < 0)
		{
			snprintf(err, err_size, "Video-Decoder fehlt (Codec nicht unterstützt).");
			goto fail;
		}
	}
	if((want & 2) && p->ai >= 0)
	{
		if(play_open_decoder(p->fc, p->ai, &p->adec) < 0)
			p->ai = -1;
	}
	// nicht gewollte Spuren gar nicht erst lesen (so laeuft jede Spur unabhaengig von der Anordnung in der Datei)
	for(unsigned int i = 0; i < p->fc->nb_streams; i++)
		if((int)i != ((want & 1) ? p->vi : -1) && (int)i != ((want & 2) ? p->ai : -1))
			p->fc->streams[i]->discard = AVDISCARD_ALL;
	int w = p->vi >= 0 ? p->fc->streams[p->vi]->codecpar->width : 0, h = p->vi >= 0 ? p->fc->streams[p->vi]->codecpar->height : 0;
	if(w <= 0 || h <= 0)
	{
		snprintf(err, err_size, "Ungültige Bildgröße.");
		goto fail;
	}
	if(max_w < 160) max_w = 160;
	if(w > max_w)
	{
		h = (int)((int64_t)h * max_w / w);
		w = max_w;
	}
	p->out_w = w & ~1;
	p->out_h = h & ~1;
	if(p->out_w < 2) p->out_w = 2;
	if(p->out_h < 2) p->out_h = 2;
	p->duration_ms = p->fc->duration > 0 ? p->fc->duration / 1000 : 0;
	p->pkt = av_packet_alloc();
	p->frame = av_frame_alloc();
	if(!p->pkt || !p->frame)
		goto fail;
	return p;
fail:
	ol_play_close(p);
	return NULL;
}

void ol_play_info(OlPlayer *p, int *w, int *h, int64_t *duration_ms, int *has_audio)
{
	if(w) *w = p->out_w;
	if(h) *h = p->out_h;
	if(duration_ms) *duration_ms = p->duration_ms;
	if(has_audio) *has_audio = p->has_audio;
}

static int play_convert_video(OlPlayer *p, uint8_t *rgba, size_t cap)
{
	AVFrame *f = p->frame;
	if((size_t)p->out_w * p->out_h * 4 > cap)
		return -1;
	if(!p->sws || p->src_w != f->width || p->src_h != f->height || p->src_fmt != f->format)
	{
		sws_freeContext(p->sws);
		p->sws = sws_getContext(f->width, f->height, f->format, p->out_w, p->out_h, AV_PIX_FMT_RGBA, SWS_BILINEAR, NULL, NULL, NULL);
		p->src_w = f->width; p->src_h = f->height; p->src_fmt = f->format;
		if(!p->sws) return -1;
	}
	uint8_t *dst[4] = { rgba, NULL, NULL, NULL };
	int stride[4] = { p->out_w * 4, 0, 0, 0 };
	sws_scale(p->sws, (const uint8_t *const *)f->data, f->linesize, 0, f->height, dst, stride);
	return 0;
}

static int play_convert_audio(OlPlayer *p, int16_t *aud, size_t cap, size_t *n)
{
	AVFrame *f = p->frame;
	if(!p->swr)
	{
		AVChannelLayout out;
		av_channel_layout_default(&out, 2);
		if(swr_alloc_set_opts2(&p->swr, &out, AV_SAMPLE_FMT_S16, 48000, &f->ch_layout, (enum AVSampleFormat)f->format, f->sample_rate, 0, NULL) < 0 || swr_init(p->swr) < 0)
		{
			swr_free(&p->swr);
			return -1;
		}
	}
	int max_frames = (int)(cap / 2);
	uint8_t *out[1] = { (uint8_t *)aud };
	int got = swr_convert(p->swr, out, max_frames, (const uint8_t **)f->extended_data, f->nb_samples);
	if(got < 0) return -1;
	*n = (size_t)got * 2;
	return 0;
}

// Rueckgabe: 1 = Videobild, 2 = Ton, 0 = nichts (weitermachen), -1 = Ende, -2 = Fehler
int ol_play_step(OlPlayer *p, uint8_t *rgba, size_t rgba_cap, int64_t *vpts_ms, int16_t *aud, size_t aud_cap, size_t *aud_n, int64_t *apts_ms)
{
	*aud_n = 0;
	for(;;)
	{
		int r = p->vdec ? avcodec_receive_frame(p->vdec, p->frame) : -1;
		if(r == 0)
		{
			int64_t ts = p->frame->best_effort_timestamp;
			AVRational tb = p->fc->streams[p->vi]->time_base;
			*vpts_ms = ts == AV_NOPTS_VALUE ? -1 : av_rescale_q(ts, tb, (AVRational){1, 1000});
			int ok = play_convert_video(p, rgba, rgba_cap);
			av_frame_unref(p->frame);
			return ok < 0 ? -2 : 1;
		}
		if(p->adec)
		{
			r = avcodec_receive_frame(p->adec, p->frame);
			if(r == 0)
			{
				int64_t ts = p->frame->best_effort_timestamp;
				AVRational tb = p->fc->streams[p->ai]->time_base;
				*apts_ms = ts == AV_NOPTS_VALUE ? -1 : av_rescale_q(ts, tb, (AVRational){1, 1000});
				int ok = play_convert_audio(p, aud, aud_cap, aud_n);
				av_frame_unref(p->frame);
				return ok < 0 ? -2 : 2;
			}
		}
		if(p->draining)
			return -1;
		r = av_read_frame(p->fc, p->pkt);
		if(r < 0)
		{
			p->draining = 1;
			if(p->vdec) avcodec_send_packet(p->vdec, NULL);
			if(p->adec) avcodec_send_packet(p->adec, NULL);
			continue;
		}
		if(p->vdec && p->pkt->stream_index == p->vi)
			avcodec_send_packet(p->vdec, p->pkt);
		else if(p->adec && p->pkt->stream_index == p->ai)
			avcodec_send_packet(p->adec, p->pkt);
		av_packet_unref(p->pkt);
	}
}

int ol_play_seek(OlPlayer *p, int64_t ms)
{
	if(ms < 0) ms = 0;
	if(av_seek_frame(p->fc, -1, ms * 1000, AVSEEK_FLAG_BACKWARD) < 0)
		return -1;
	if(p->vdec) avcodec_flush_buffers(p->vdec);
	if(p->adec) avcodec_flush_buffers(p->adec);
	if(p->swr) swr_free(&p->swr);
	p->draining = 0;
	return 0;
}

void ol_play_close(OlPlayer *p)
{
	if(!p) return;
	if(p->sws) sws_freeContext(p->sws);
	if(p->swr) swr_free(&p->swr);
	if(p->vdec) avcodec_free_context(&p->vdec);
	if(p->adec) avcodec_free_context(&p->adec);
	if(p->pkt) av_packet_free(&p->pkt);
	if(p->frame) av_frame_free(&p->frame);
	if(p->fc) avformat_close_input(&p->fc);
	free(p);
}

// Vorschaubild: ein Bild nach etwa einer Sekunde, passend in max_w x max_h verkleinert.
int ol_play_thumb(const char *path, int max_w, uint8_t *rgba, size_t cap, int *w, int *h, int64_t *duration_ms)
{
	char err[128];
	OlPlayer *p = ol_play_open(path, max_w, 1, err, sizeof(err));
	if(!p) return -1;
	if(duration_ms) *duration_ms = p->duration_ms;
	if(p->duration_ms > 3000)
		ol_play_seek(p, p->duration_ms > 20000 ? 5000 : 1500);
	int res = -1;
	int16_t aud[16384];
	for(int i = 0; i < 400; i++)
	{
		int64_t vp = 0, ap = 0;
		size_t an = 0;
		int r = ol_play_step(p, rgba, cap, &vp, aud, 16384, &an, &ap);
		if(r == 1) { res = 0; break; }
		if(r < 0) break;
	}
	if(res == 0) { *w = p->out_w; *h = p->out_h; }
	ol_play_close(p);
	return res;
}


// ---------------------------------------------------------------- Mikrofon (Sprache zur Konsole)
// Rueckgabe: 0 = ok, -1 = nicht bereit
int ol_mic_enable(OlStream *s, int on)
{
	if(!s || !s->mic_ready)
		return -1;
	if(on && !s->mic_connected)
	{
		chiaki_session_connect_microphone(&s->session);
		s->mic_connected = 1;
	}
	if(s->mic_connected)
		chiaki_session_toggle_microphone(&s->session, on ? true : false);
	s->mic_live = on ? 1 : 0;
	return 0;
}

// genau 480 Stereo-Abtastwerte (960 int16, 48 kHz) = 10 ms
void ol_mic_push(OlStream *s, const int16_t *pcm)
{
	if(!s || !s->mic_ready || !s->mic_live)
		return;
	chiaki_opus_encoder_frame((int16_t *)pcm, &s->mic_enc);
}


// ---------------------------------------------------------------- Opus fuer den Sprachchat (48 kHz, mono)
void *ol_opus_enc_new(void)
{
	int err = 0;
	OpusEncoder *e = opus_encoder_create(48000, 1, OPUS_APPLICATION_VOIP, &err);
	if(!e || err != OPUS_OK) return NULL;
	opus_encoder_ctl(e, OPUS_SET_BITRATE(28000));
	opus_encoder_ctl(e, OPUS_SET_SIGNAL(OPUS_SIGNAL_VOICE));
	opus_encoder_ctl(e, OPUS_SET_INBAND_FEC(1));
	opus_encoder_ctl(e, OPUS_SET_PACKET_LOSS_PERC(8));
	opus_encoder_ctl(e, OPUS_SET_DTX(0));
	return e;
}

int ol_opus_enc(void *e, const int16_t *pcm, int frame, uint8_t *out, int cap)
{
	return opus_encode((OpusEncoder *)e, pcm, frame, out, cap);
}

void ol_opus_enc_free(void *e)
{
	if(e) opus_encoder_destroy((OpusEncoder *)e);
}

void *ol_opus_dec_new(void)
{
	int err = 0;
	OpusDecoder *d = opus_decoder_create(48000, 1, &err);
	if(!d || err != OPUS_OK) return NULL;
	return d;
}

// data == NULL: Paketverlust ausgleichen
int ol_opus_dec(void *d, const uint8_t *data, int len, int16_t *pcm, int frame)
{
	return opus_decode((OpusDecoder *)d, data, data ? len : 0, pcm, frame, 0);
}

void ol_opus_dec_free(void *d)
{
	if(d) opus_decoder_destroy((OpusDecoder *)d);
}


// ---------------------------------------------------------------- Replay-Clips
// secs = 0: aus. Sonst werden die letzten secs Sekunden im Speicher gehalten.
int ol_replay_enable(OlStream *s, int secs)
{
	if(!s) return -1;
	if(secs > 120) secs = 120;
	chiaki_mutex_lock(&s->rp_mtx);
	rp_clear_locked(s);
	s->rp_secs = secs > 0 ? secs : 0;
	if(secs > 0)
	{
		s->rp_au_cap = (secs + 8) * 130 + 64;
		s->rp_aud_cap = (secs + 8) * 260 + 64;
		s->rp_au = (struct RpAu *)calloc((size_t)s->rp_au_cap, sizeof(struct RpAu));
		s->rp_aud = (struct RpAud *)calloc((size_t)s->rp_aud_cap, sizeof(struct RpAud));
		if(!s->rp_au || !s->rp_aud)
		{
			rp_clear_locked(s);
			s->rp_secs = 0;
			chiaki_mutex_unlock(&s->rp_mtx);
			return -2;
		}
	}
	chiaki_mutex_unlock(&s->rp_mtx);
	return 0;
}

// wie viele Millisekunden gerade im Puffer liegen
int ol_replay_buffered_ms(OlStream *s)
{
	if(!s) return 0;
	chiaki_mutex_lock(&s->rp_mtx);
	int ms = 0;
	if(s->rp_au_cnt > 0)
		ms = (int)((now_us() - s->rp_au[s->rp_au_head].t) / 1000);
	chiaki_mutex_unlock(&s->rp_mtx);
	return ms;
}

// Speichert die letzten Sekunden als MP4. 0 = ok.
int ol_replay_save(OlStream *s, const char *path, char *err, size_t err_size)
{
	if(!s) return -1;
	err[0] = 0;
	chiaki_mutex_lock(&s->rp_mtx);
	if(s->rp_secs <= 0 || s->rp_au_cnt == 0)
	{
		chiaki_mutex_unlock(&s->rp_mtx);
		snprintf(err, err_size, "Der Replay-Puffer ist leer.");
		return 1;
	}
	uint64_t now = now_us();
	uint64_t want = now - (uint64_t)s->rp_secs * 1000000ULL;
	// erstes Schluesselbild im Zeitfenster, sonst das aelteste vorhandene
	int first = -1, oldest_key = -1;
	for(int i = 0; i < s->rp_au_cnt; i++)
	{
		struct RpAu *a = &s->rp_au[(s->rp_au_head + i) % s->rp_au_cap];
		if(!a->key) continue;
		if(oldest_key < 0) oldest_key = i;
		if(a->t >= want) { first = i; break; }
	}
	if(first < 0) first = oldest_key;
	if(first < 0)
	{
		chiaki_mutex_unlock(&s->rp_mtx);
		snprintf(err, err_size, "Noch kein Schluesselbild im Puffer.");
		return 2;
	}
	int n = s->rp_au_cnt - first;
	struct RpAu *au = (struct RpAu *)calloc((size_t)n, sizeof(struct RpAu));
	if(!au)
	{
		chiaki_mutex_unlock(&s->rp_mtx);
		return 3;
	}
	for(int i = 0; i < n; i++)
	{
		struct RpAu *src = &s->rp_au[(s->rp_au_head + first + i) % s->rp_au_cap];
		au[i] = *src;
		au[i].d = (uint8_t *)malloc(src->n);
		if(au[i].d) memcpy(au[i].d, src->d, src->n);
	}
	uint64_t t0 = au[0].t;
	int an = 0;
	struct RpAud *aud = (struct RpAud *)calloc((size_t)s->rp_aud_cnt + 1, sizeof(struct RpAud));
	for(int i = 0; i < s->rp_aud_cnt && aud; i++)
	{
		struct RpAud *src = &s->rp_aud[(s->rp_aud_head + i) % s->rp_aud_cap];
		if(src->t < t0) continue;
		aud[an] = *src;
		aud[an].d = (int16_t *)malloc(src->frames * src->ch * sizeof(int16_t));
		if(aud[an].d) memcpy(aud[an].d, src->d, src->frames * src->ch * sizeof(int16_t));
		an++;
	}
	// Daten fuer den Schreiber
	int hevc = s->codec_hevc, w = s->width, h = s->height;
	uint8_t *hdr = NULL;
	size_t hdr_len = s->vhdr_len;
	if(s->vhdr && hdr_len)
	{
		hdr = (uint8_t *)malloc(hdr_len);
		if(hdr) memcpy(hdr, s->vhdr, hdr_len);
		else hdr_len = 0;
	}
	chiaki_mutex_unlock(&s->rp_mtx);

	// Mit einem eigenen kleinen Aufnahme-Zustand schreiben (gleicher Weg wie die normale Aufnahme)
	OlStream *t = (OlStream *)calloc(1, sizeof(*t));
	int rc = 0;
	if(!t) { rc = 4; goto done; }
	chiaki_mutex_init(&t->mtx, false);
	chiaki_mutex_init(&t->rec_mtx, false);
	chiaki_mutex_init(&t->rp_mtx, false);
	t->codec_hevc = hevc;
	t->width = w;
	t->height = h;
	t->vhdr = hdr; t->vhdr_len = hdr_len; hdr = NULL;
	t->rec_use_ts = 1;
	rc = rec_open(t, path, err, err_size);
	if(rc != 0) goto done;
	int ai = 0;
	for(int i = 0; i < n; i++)
	{
		// Ton bis zu diesem Bild zuerst schreiben (nur wenn die Aufnahme schon laeuft)
		if(i > 0)
		{
			while(ai < an && aud[ai].t <= au[i].t)
			{
				if(aud[ai].d) rec_audio(t, aud[ai].d, aud[ai].frames, aud[ai].ch);
				ai++;
			}
		}
		else
		{
			while(ai < an && aud[ai].t < au[0].t) ai++;
		}
		t->rec_ts_us = au[i].t;
		if(au[i].d) rec_video(t, au[i].d, au[i].n);
	}
	while(ai < an) { if(aud[ai].d) rec_audio(t, aud[ai].d, aud[ai].frames, aud[ai].ch); ai++; }
	ol_record_stop(t);
done:
	if(t)
	{
		free(t->vhdr);
		free(t);
	}
	free(hdr);
	for(int i = 0; i < n; i++) free(au[i].d);
	free(au);
	for(int i = 0; i < an; i++) free(aud[i].d);
	free(aud);
	return rc;
}


// Selbsttest: Eingabe-Datei (Annex-B mit AUD) im Echtzeit-Takt in den Replay-Puffer geben und die letzten secs Sekunden speichern
int ol_replay_selftest(const char *in_path, const char *out_path, int hevc, int w, int h, int secs, char *err, size_t err_size)
{
	FILE *f = fopen(in_path, "rb");
	if(!f) { snprintf(err, err_size, "Eingabe fehlt"); return 10; }
	fseek(f, 0, SEEK_END);
	long n = ftell(f);
	fseek(f, 0, SEEK_SET);
	uint8_t *buf = malloc((size_t)n);
	if(!buf || fread(buf, 1, (size_t)n, f) != (size_t)n) { fclose(f); return 11; }
	fclose(f);
	OlStream *s = calloc(1, sizeof(*s));
	chiaki_mutex_init(&s->mtx, false);
	chiaki_mutex_init(&s->rec_mtx, false);
	chiaki_mutex_init(&s->rp_mtx, false);
	s->codec_hevc = hevc;
	s->width = w;
	s->height = h;
	ol_replay_enable(s, secs);
	long *starts = malloc(sizeof(long) * 100001);
	int cnt = 0;
	for(long i = 0; i + 4 < n && cnt < 99999; i++)
	{
		if(buf[i] == 0 && buf[i + 1] == 0 && buf[i + 2] == 1)
		{
			int t = hevc ? ((buf[i + 3] >> 1) & 0x3f) : (buf[i + 3] & 0x1f);
			if(t == (hevc ? 35 : 9))
				starts[cnt++] = (i > 0 && buf[i - 1] == 0) ? i - 1 : i;
		}
	}
	starts[cnt] = n;
	int16_t tone[2 * 800];
	double ph = 0;
	for(int k = 0; k < cnt; k++)
	{
		const uint8_t *au = buf + starts[k];
		size_t aun = (size_t)(starts[k + 1] - starts[k]);
		size_t sp = 0;
		if(k == 0)
		{
			for(size_t q = 0; q + 3 < aun; q++)
			{
				if(au[q] == 0 && au[q + 1] == 0 && au[q + 2] == 1)
				{
					int t = hevc ? ((au[q + 3] >> 1) & 0x3f) : (au[q + 3] & 0x1f);
					if(hevc ? (t >= 16 && t <= 21) : (t == 5)) { sp = (q > 0 && au[q - 1] == 0) ? q - 1 : q; break; }
				}
			}
		}
		if(sp > 0)
		{
			rec_video(s, au, sp);              // Parameter-Sets merken
			rp_push_video(s, au + sp, aun - sp);
		}
		else
			rp_push_video(s, au, aun);
		for(int i = 0; i < 800; i++)
		{
			int16_t v = (int16_t)(sin(ph) * 8000);
			ph += 2.0 * 3.14159265358979 * 440.0 / 48000.0;
			tone[2 * i] = v; tone[2 * i + 1] = v;
		}
		rp_push_audio(s, tone, 800, 2);
		Sleep(33);
	}
	err[0] = 0;
	int rc = ol_replay_save(s, out_path, err, err_size);
	free(starts);
	free(buf);
	return rc;
}


// Konsole in den Ruhemodus schicken (wie "PS5 ausschalten" im Remote-Play-Menue)
int ol_stream_standby(OlStream *s)
{
	if(!s) return -1;
	return chiaki_session_goto_bed(&s->session) == CHIAKI_ERR_SUCCESS ? 0 : -1;
}


// ---------------------------------------------------------------- Video zuschneiden (ohne neu zu kodieren)
// Schneidet [start_ms, end_ms] aus einer MP4 und schreibt eine neue. Der Anfang rutscht auf das davorliegende Schluesselbild.
int ol_trim(const char *in_path, const char *out_path, int64_t start_ms, int64_t end_ms, char *err, size_t err_size)
{
	AVFormatContext *ic = NULL, *oc = NULL;
	int *map = NULL;
	AVPacket *pkt = NULL;
	int rc = 1;
	err[0] = 0;
	if(avformat_open_input(&ic, in_path, NULL, NULL) < 0)
	{
		snprintf(err, err_size, "Die Datei laesst sich nicht oeffnen.");
		return 1;
	}
	if(avformat_find_stream_info(ic, NULL) < 0)
	{
		snprintf(err, err_size, "Die Datei ist beschaedigt.");
		goto done;
	}
	if(avformat_alloc_output_context2(&oc, NULL, "mp4", out_path) < 0 || !oc)
	{
		snprintf(err, err_size, "MP4 nicht moeglich.");
		goto done;
	}
	map = (int *)calloc(ic->nb_streams, sizeof(int));
	int n_out = 0, vi = -1;
	for(unsigned int i = 0; i < ic->nb_streams; i++)
	{
		AVStream *st = ic->streams[i];
		map[i] = -1;
		if(st->codecpar->codec_type != AVMEDIA_TYPE_VIDEO && st->codecpar->codec_type != AVMEDIA_TYPE_AUDIO)
			continue;
		AVStream *os = avformat_new_stream(oc, NULL);
		if(!os || avcodec_parameters_copy(os->codecpar, st->codecpar) < 0)
		{
			snprintf(err, err_size, "Stream nicht moeglich.");
			goto done;
		}
		os->codecpar->codec_tag = 0;
		os->time_base = st->time_base;
		map[i] = n_out++;
		if(st->codecpar->codec_type == AVMEDIA_TYPE_VIDEO && vi < 0) vi = (int)i;
	}
	if(vi < 0)
	{
		snprintf(err, err_size, "Kein Videobild.");
		goto done;
	}
	if(avio_open(&oc->pb, out_path, AVIO_FLAG_WRITE) < 0)
	{
		snprintf(err, err_size, "Datei kann nicht geschrieben werden.");
		goto done;
	}
	AVDictionary *opts = NULL;
	av_dict_set(&opts, "movflags", "frag_keyframe+empty_moov+default_base_moof", 0);
	int hr = avformat_write_header(oc, &opts);
	av_dict_free(&opts);
	if(hr < 0)
	{
		snprintf(err, err_size, "Kopfdaten nicht moeglich.");
		goto done;
	}
	if(start_ms < 0) start_ms = 0;
	if(ic->duration > 0 && start_ms * 1000 >= ic->duration)
	{
		snprintf(err, err_size, "Der Anfang liegt hinter dem Ende des Videos.");
		goto done;
	}
	av_seek_frame(ic, -1, start_ms * 1000, AVSEEK_FLAG_BACKWARD);
	int *fin = (int *)calloc(ic->nb_streams, sizeof(int));
	pkt = av_packet_alloc();
	int64_t base_ms = -1;
	int wrote = 0;
	while(av_read_frame(ic, pkt) >= 0)
	{
		int idx = pkt->stream_index;
		if(idx < 0 || map[idx] < 0) { av_packet_unref(pkt); continue; }
		AVStream *st = ic->streams[idx];
		int64_t ts = pkt->pts != AV_NOPTS_VALUE ? pkt->pts : pkt->dts;
		int64_t t_ms = ts == AV_NOPTS_VALUE ? -1 : av_rescale_q(ts, st->time_base, (AVRational){1, 1000});
		if(idx == vi)
		{
			if(base_ms < 0)
			{
				if(!(pkt->flags & AV_PKT_FLAG_KEY) || t_ms < 0) { av_packet_unref(pkt); continue; }
				base_ms = t_ms;
			}
			if(t_ms > end_ms) { fin[idx] = 1; av_packet_unref(pkt); goto check_done; }
		}
		else
		{
			if(base_ms >= 0 && t_ms > end_ms) { fin[idx] = 1; av_packet_unref(pkt); goto check_done; }
			if(base_ms < 0 || t_ms < base_ms) { av_packet_unref(pkt); continue; }
		}
		int64_t off = av_rescale_q(base_ms, (AVRational){1, 1000}, st->time_base);
		if(pkt->pts != AV_NOPTS_VALUE) pkt->pts -= off;
		if(pkt->dts != AV_NOPTS_VALUE) pkt->dts -= off;
		pkt->stream_index = map[idx];
		pkt->pos = -1;
		if(av_interleaved_write_frame(oc, pkt) < 0) { av_packet_unref(pkt); break; }
		wrote++;
		av_packet_unref(pkt);
		continue;
check_done:
		{
			int all = 1;
			for(unsigned int q = 0; q < ic->nb_streams; q++)
				if(map[q] >= 0 && !fin[q]) all = 0;
			if(all) break;
		}
	}
	free(fin);
	av_write_trailer(oc);
	if(wrote == 0)
	{
		snprintf(err, err_size, "Im gewaehlten Bereich ist nichts.");
		goto done;
	}
	rc = 0;
done:
	if(pkt) av_packet_free(&pkt);
	free(map);
	if(oc)
	{
		if(oc->pb) avio_closep(&oc->pb);
		avformat_free_context(oc);
	}
	if(ic) avformat_close_input(&ic);
	if(rc != 0) remove(out_path);
	return rc;
}
