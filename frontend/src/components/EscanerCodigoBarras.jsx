import { useEffect, useRef, useState } from 'react';
import { BrowserMultiFormatReader } from '@zxing/browser';
import './EscanerCodigoBarras.css';

// Tiempo mínimo entre dos detecciones del MISMO código antes de volver a
// dispararlo. Sin esto, la cámara ve el mismo código en decenas de
// fotogramas por segundo mientras no se mueve, y lo agregaría al carrito
// muchas veces de golpe con un solo pase de escaneo.
const ENFRIAMIENTO_MISMO_CODIGO_MS = 1500;

function reproducirBeep() {
  try {
    const Contexto = window.AudioContext || window.webkitAudioContext;
    if (!Contexto) return;
    const contexto = new Contexto();
    const oscilador = contexto.createOscillator();
    const ganancia = contexto.createGain();
    oscilador.connect(ganancia);
    ganancia.connect(contexto.destination);
    oscilador.frequency.value = 880;
    ganancia.gain.setValueAtTime(0.15, contexto.currentTime);
    oscilador.start();
    oscilador.stop(contexto.currentTime + 0.12);
  } catch {
    // Sin audio disponible (navegador raro, permisos, etc.) — no es
    // crítico para la función principal del escáner.
  }
}

/**
 * Escáner de código de barras vía cámara. No decide qué hacer con el
 * código detectado — solo lo detecta, lo depura del ruido de fotogramas
 * repetidos, y avisa al padre vía `onCodigoDetectado`. El padre (POS o
 * Inventario) es quien decide qué hacer con cada código.
 *
 * IMPORTANTE: el elemento <video> se crea de forma IMPERATIVA (con
 * document.createElement) en vez de vivir fijo en el JSX. Esto evita un
 * problema real de React 18 en modo desarrollo: React monta los
 * componentes dos veces seguidas (StrictMode) para detectar bugs, y dos
 * arranques de cámara casi simultáneos sobre el MISMO elemento <video>
 * fijo terminaban pisándose entre sí, dejando el video sin imagen
 * (pantalla negra) aunque el permiso de cámara sí se había concedido. Al
 * crear un <video> nuevo en cada intento, cada uno queda aislado del
 * otro y no hay forma de que se interfieran.
 */
export default function EscanerCodigoBarras({
  onCodigoDetectado,
  onCerrar,
  ultimoResultado,
  cerrarAlDetectar = false,
}) {
  const contenedorVideoRef = useRef(null);
  const ultimoCodigoRef = useRef({ codigo: null, timestamp: 0 });
  const [error, setError] = useState(null);

  useEffect(() => {
    let cancelado = false;
    let controles = null;

    const videoEl = document.createElement('video');
    videoEl.className = 'escaner-video';
    videoEl.muted = true;
    videoEl.setAttribute('playsinline', 'true');
    videoEl.setAttribute('autoplay', 'true');
    contenedorVideoRef.current?.appendChild(videoEl);

    const lector = new BrowserMultiFormatReader();

    lector
      .decodeFromConstraints(
        { video: { facingMode: { ideal: 'environment' } } },
        videoEl,
        (resultado) => {
          if (!resultado) return; // sin código en este fotograma, normal
          if (cancelado) return;

          const codigo = resultado.getText();
          const ahora = Date.now();
          const { codigo: ultimoCodigo, timestamp } = ultimoCodigoRef.current;

          if (codigo === ultimoCodigo && ahora - timestamp < ENFRIAMIENTO_MISMO_CODIGO_MS) {
            return; // mismo código visto hace muy poco — se ignora
          }

          ultimoCodigoRef.current = { codigo, timestamp: ahora };
          reproducirBeep();
          onCodigoDetectado(codigo);

          if (cerrarAlDetectar) {
            onCerrar();
          }
        }
      )
      .then((ctrl) => {
        if (cancelado) {
          ctrl.stop();
          videoEl.remove();
          return;
        }
        controles = ctrl;
      })
      .catch((e) => {
        videoEl.remove();
        if (cancelado) return;
        setError(
          e?.name === 'NotAllowedError'
            ? 'Se necesita permiso de cámara para escanear. Revisa los permisos del navegador para este sitio.'
            : 'No se pudo acceder a la cámara. Verifica que el dispositivo tenga una disponible y que no esté en uso por otra app.'
        );
      });

    return () => {
      cancelado = true;
      controles?.stop();
      videoEl.remove();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="escaner-overlay">
      <div className="escaner-modal">
        <div className="escaner-header">
          <h2>Escanear código de barras</h2>
          <button type="button" className="escaner-cerrar" onClick={onCerrar} aria-label="Cerrar">
            ×
          </button>
        </div>

        <div className="escaner-video-wrapper">
          {error ? (
            <p className="escaner-error">{error}</p>
          ) : (
            <>
              <div ref={contenedorVideoRef} className="escaner-video-contenedor" />
              <div className="escaner-marco" />
            </>
          )}
        </div>

        {ultimoResultado && (
          <p className={`escaner-resultado escaner-resultado-${ultimoResultado.tipo}`}>
            {ultimoResultado.tipo === 'ok' ? '✓ ' : '⚠ '}
            {ultimoResultado.texto}
          </p>
        )}

        <p className="escaner-ayuda">
          {cerrarAlDetectar
            ? 'Apunta al código de barras del producto.'
            : 'Sigue escaneando — cada producto se agrega solo al carrito.'}
        </p>

        {!cerrarAlDetectar && (
          <button type="button" className="escaner-listo" onClick={onCerrar}>
            Listo
          </button>
        )}
      </div>
    </div>
  );
}