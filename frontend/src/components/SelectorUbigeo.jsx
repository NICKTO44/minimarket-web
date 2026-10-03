import { useEffect, useMemo, useRef, useState } from 'react';
import './SelectorUbigeo.css';

// Los 1874 distritos se cargan solo cuando hace falta (una vez por sesión).
let promesaUbigeos = null;
function cargarUbigeos() {
  if (!promesaUbigeos) {
    promesaUbigeos = import('../data/ubigeos').then((m) => m.UBIGEOS);
  }
  return promesaUbigeos;
}

const sinTildes = (t) =>
  String(t)
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase();

const textoDe = (u) => `${u.distrito}, ${u.provincia}, ${u.departamento}`;

/**
 * Elegir un distrito del Perú escribiendo su nombre ("san seb" → San
 * Sebastian, Cusco, Cusco). Devuelve el ubigeo INEI de 6 dígitos, que es el
 * que pide SUNAT en la guía de remisión. Si el distrito no está en la lista
 * (creado hace poco), se puede escribir el código directamente.
 */
export default function SelectorUbigeo({ valor, onCambiar, placeholder = 'Escribe el distrito...', etiqueta = 'Distrito' }) {
  const [ubigeos, setUbigeos] = useState(null);
  const [texto, setTexto] = useState('');
  const [abierto, setAbierto] = useState(false);
  const cierre = useRef(null);

  useEffect(() => {
    let vivo = true;
    cargarUbigeos()
      .then((u) => vivo && setUbigeos(u))
      .catch(() => vivo && setUbigeos([]));
    return () => {
      vivo = false;
      clearTimeout(cierre.current);
    };
  }, []);

  const elegido = useMemo(() => (valor && ubigeos ? ubigeos.find((u) => u.ubigeo === valor) : null), [valor, ubigeos]);

  const resultados = useMemo(() => {
    const buscado = sinTildes(texto.trim());
    if (!ubigeos || buscado.length < 2) return [];
    const palabras = buscado.split(/[\s,]+/).filter(Boolean);
    const encontrados = [];
    for (const u of ubigeos) {
      const donde = sinTildes(`${u.distrito} ${u.provincia} ${u.departamento} ${u.ubigeo}`);
      if (palabras.every((p) => donde.includes(p))) {
        // Primero los que empiezan por lo escrito.
        encontrados.push({ u, orden: sinTildes(u.distrito).startsWith(palabras[0]) ? 0 : 1 });
        if (encontrados.length >= 60) break;
      }
    }
    return encontrados
      .sort((a, b) => a.orden - b.orden)
      .slice(0, 8)
      .map((e) => e.u);
  }, [texto, ubigeos]);

  const elegir = (u) => {
    onCambiar(u.ubigeo);
    setTexto('');
    setAbierto(false);
  };

  // Código escrito a mano (6 dígitos) que no está en la lista.
  const codigoDirecto = /^\d{6}$/.test(texto.trim()) && !resultados.some((u) => u.ubigeo === texto.trim()) ? texto.trim() : null;

  if (valor && !abierto) {
    return (
      <div className="ubi-elegido">
        <span>{elegido ? `${textoDe(elegido)}` : `Ubigeo ${valor}`}</span>
        <button
          type="button"
          onClick={() => {
            onCambiar('');
            setAbierto(true);
          }}
          aria-label={`Cambiar ${etiqueta.toLowerCase()}`}
        >
          Cambiar
        </button>
      </div>
    );
  }

  return (
    <div className="ubi-selector">
      <input
        value={texto}
        placeholder={ubigeos ? placeholder : 'Cargando distritos...'}
        onChange={(e) => {
          setTexto(e.target.value);
          setAbierto(true);
        }}
        onFocus={() => setAbierto(true)}
        // Se cierra con un respiro para que alcance a registrarse el clic en una opción.
        onBlur={() => (cierre.current = setTimeout(() => setAbierto(false), 180))}
        aria-label={etiqueta}
        autoComplete="off"
      />
      {abierto && (resultados.length > 0 || codigoDirecto) && (
        <div className="ubi-lista" role="listbox">
          {resultados.map((u) => (
            <button type="button" key={u.ubigeo} role="option" aria-selected="false" onMouseDown={(e) => e.preventDefault()} onClick={() => elegir(u)}>
              <strong>{u.distrito}</strong>
              <span>
                {u.provincia}, {u.departamento}
              </span>
            </button>
          ))}
          {codigoDirecto && (
            <button type="button" role="option" aria-selected="false" onMouseDown={(e) => e.preventDefault()} onClick={() => elegir({ ubigeo: codigoDirecto })}>
              <strong>Usar el código {codigoDirecto}</strong>
              <span>No está en la lista</span>
            </button>
          )}
        </div>
      )}
      {abierto && ubigeos && texto.trim().length >= 2 && resultados.length === 0 && !codigoDirecto && (
        <div className="ubi-lista">
          <p>No se encontró. Escribe el nombre del distrito o su código de 6 dígitos.</p>
        </div>
      )}
    </div>
  );
}
