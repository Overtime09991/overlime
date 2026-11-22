#include "overlime_core.h"

#include <chiaki/session.h>
#include <chiaki/regist.h>
#include <chiaki/discovery.h>
#include <chiaki/ffmpegdecoder.h>
#include <chiaki/opusdecoder.h>
#include <chiaki/controller.h>
#include <chiaki/log.h>
#include <chiaki/thread.h>
#include <chiaki/time.h>
#include <windows.h>

#include <libswscale/swscale.h>
#include <libavutil/imgutils.h>
#include <libavutil/hwcontext.h>
#include <libavutil/pixfmt.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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

struct OlStream {
	ChiakiLog log;
	ChiakiSession session;
	ChiakiFfmpegDecoder decoder;
	ChiakiOpusDecoder opus;
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
	int sws_w, sws_h, sws_fmt;

	// Vibration
	uint8_t rumble_l, rumble_r;
	int rumble_new;

	// Audio
	int16_t audio[AUDIO_RING_SAMPLES];
	size_t a_read, a_write; // monoton steigend
	uint32_t audio_rate, audio_channels;
};

static void frame_available(ChiakiFfmpegDecoder *decoder, void *user)
{
	OlStream *s = user;
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
			s->frames_decoded++;
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
	chiaki_mutex_lock(&s->mtx);
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

OlStream *ol_stream_start(const OlStreamParams *p, char *err, size_t err_size)
{
	ol_init();
	OlStream *s = calloc(1, sizeof(*s));
	if(!s)
		return NULL;
	make_log(&s->log);
	chiaki_mutex_init(&s->mtx, false);
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
	ci.video_profile_auto_downgrade = true;
	ci.enable_keyboard = false;
	ci.enable_dualsense = false;
	ci.packet_loss_max = 0.05;
	ci.enable_idr_on_fec_failure = true;

	ChiakiErrorCode e = chiaki_session_init(&s->session, &ci, &s->log);
	if(e != CHIAKI_ERR_SUCCESS)
	{
		snprintf(err, err_size, "Sitzung konnte nicht erstellt werden: %s", chiaki_error_string(e));
		goto fail;
	}
	s->session_ok = 1;

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
	chiaki_session_set_video_sample_cb(&s->session, chiaki_ffmpeg_decoder_video_sample_cb, &s->decoder);
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
	if(!s)
		return;
	chiaki_session_stop(&s->session);
	chiaki_session_join(&s->session);
	chiaki_opus_decoder_fini(&s->opus);
	chiaki_ffmpeg_decoder_fini(&s->decoder);
	chiaki_session_fini(&s->session);
	sws_freeContext(s->sws);
	free(s->rgba);
	chiaki_mutex_fini(&s->mtx);
	free(s);
}
