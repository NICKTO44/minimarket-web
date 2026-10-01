import { useCallback, useEffect, useRef, useState } from 'react';
import { ClipboardList, Plus, X } from 'lucide-react';
import { api } from '../../api/api';
import '../Mesas/Mesas.css';
import './CartaDia.css';

// Precio con el que nace cada plato nuevo (se puede cambiar).
const PRECIO_POR_DEFECTO = '10.00';
// Cada cuánto se trae la carta (la cocina puede marcar "Agotado").
const REFRESCO_MS = 15000;

function fechaDeHoy() {
  return new Date().toLocaleDateString('es-PE', { weekday: 'long', day: 'numeric', month: 'short' });
}

function precioValido(texto) {
  const n = Number(String(texto).replace(',', '.'));
  return Number.isFinite(n) && n > 0 && n <= 9999 ? Math.round(n * 100) / 100 : null;
}

/**
 * Un plato ya guardado. Nombre y precio se editan ahí mismo y se guardan
 * al salir del campo (o con Enter). Se vuelve a montar si cambia en el
 * servidor (ver key en CartaDia), así siempre parte de lo guardado.
 */
function FilaPlato({ plato, ocupado, onGuardar, onAgotado, onQuitar }) {
  const [nombre, setNombre] = useState(plato.nombre);
  const [precio, setPrecio] = useState(plato.precio.toFixed(2));
  const [confirmarQuitar, setConfirmarQuitar] = useState(false);

  useEffect(() => {
    if (!confirmarQuitar) return undefined;
    const t = setTimeout(() => setConfirmarQuitar(false), 3000);
    return () => clearTimeout(t);
  }, [confirmarQuitar]);

  const guardar = () => {
    const limpio = nombre.trim().replace(/\s+/g, ' ');
    const p = precioValido(precio);
    if (!limpio || p === null) {
      // Lo escrito no sirve: vuelve a lo guardado.
      setNombre(plato.nombre);
      setPrecio(plato.precio.toFixed(2));
      return;
    }
    if (limpio === plato.nombre && p === plato.precio) return;
    onGuardar(plato.id, { nombre: limpio, precio: p });
  };

  const alPresionar = (e) => {
    if (e.key === 'Enter') e.currentTarget.blur();
  };

  return (
    <li className={`carta-fila${plato.agotado ? ' carta-fila-agotada' : ''}`}>
      <input
        className="mesas-input carta-nombre"
        value={nombre}
        maxLength={80}
        onChange={(e) => setNombre(e.target.value)}
        onBlur={guardar}
        onKeyDown={alPresionar}
        aria-label="Nombre del plato"
      />
      <label className="carta-precio">
        <span>S/</span>
        <input
          className="mesas-input"
          inputMode="decimal"
          value={precio}
          onChange={(e) => setPrecio(e.target.value)}
          onFocus={(e) => e.target.select()}
          onBlur={guardar}
          onKeyDown={alPresionar}
          aria-label="Precio"
        />
      </label>
      <button
        type="button"
        className={`carta-estado ${plato.agotado ? 'agotado' : 'disponible'}`}
        disabled={ocupado}
        onClick={() => onAgotado(plato.id, !plato.agotado)}
        title={plato.agotado ? 'Tocar si volvió a haber' : 'Tocar cuando se acabe'}
      >
        {plato.agotado ? 'Agotado' : 'Disponible'}
      </button>
      {confirmarQuitar ? (
        <button type="button" className="carta-quitar confirmar" disabled={ocupado} onClick={() => onQuitar(plato.id)}>
          ¿Quitar?
        </button>
      ) : (
        <button
          type="button"
          className="carta-quitar"
          disabled={ocupado}
          onClick={() => setConfirmarQuitar(true)}
          aria-label={`Quitar ${plato.nombre}`}
        >
          <X size={16} />
        </button>
      )}
    </li>
  );
}

/**
 * "Carta de hoy": cada mañana el mozo (o cocina, o el administrador)
 * escribe los platos del día. Aparecen primero en Mesas y en el POS, y al
 * día siguiente desaparecen solos. "Agotado" los deja en gris y ya no se
 * pueden pedir. Las bebidas y extras siguen siendo productos normales.
 */
export default function CartaDia() {
  const [platos, setPlatos] = useState([]);
  const [cargando, setCargando] = useState(true);
  const [error, setError] = useState(null);
  const [ocupado, setOcupado] = useState(false);
  const [nuevoNombre, setNuevoNombre] = useState('');
  const [nuevoPrecio, setNuevoPrecio] = useState(PRECIO_POR_DEFECTO);
  const [fecha] = useState(fechaDeHoy);
  const nombreNuevoRef = useRef(null);

  const cargar = useCallback(
    () =>
      api
        .cartaDia()
        .then((p) => {
          setPlatos(p);
          setError(null);
        })
        .catch((e) => setError(e.message))
        .finally(() => setCargando(false)),
    []
  );

  useEffect(() => {
    cargar();
    const intervalo = setInterval(() => {
      // No se refresca mientras se escribe, para no mover la pantalla.
      if (!document.activeElement?.closest?.('.carta-layout input')) cargar();
    }, REFRESCO_MS);
    return () => clearInterval(intervalo);
  }, [cargar]);

  const ejecutar = async (accion) => {
    setOcupado(true);
    try {
      setPlatos(await accion());
      setError(null);
      return true;
    } catch (e) {
      setError(e.message);
      return false;
    } finally {
      setOcupado(false);
    }
  };

  const agregar = async (e) => {
    e.preventDefault();
    const nombre = nuevoNombre.trim().replace(/\s+/g, ' ');
    const precio = precioValido(nuevoPrecio);
    if (!nombre) {
      nombreNuevoRef.current?.focus();
      return;
    }
    if (precio === null) {
      setError('Escribe un precio válido (mayor a 0).');
      return;
    }
    const ok = await ejecutar(() => api.cartaAgregar(nombre, precio));
    if (ok) {
      setNuevoNombre('');
      setNuevoPrecio(PRECIO_POR_DEFECTO);
    }
    // Listo para escribir el siguiente plato.
    nombreNuevoRef.current?.focus();
  };

  const agotados = platos.filter((p) => p.agotado).length;

  return (
    <div className="carta-layout">
      <header className="carta-header">
        <div>
          <h1>
            <ClipboardList size={22} /> Carta de hoy
          </h1>
          <p className="carta-fecha">{fecha}</p>
        </div>
        {platos.length > 0 && (
          <span className="carta-resumen">
            {platos.length} {platos.length === 1 ? 'plato' : 'platos'}
            {agotados > 0 && ` · ${agotados} ${agotados === 1 ? 'agotado' : 'agotados'}`}
          </span>
        )}
      </header>

      <p className="carta-ayuda">
        Escribe los platos del día. Aparecen primero al tomar pedidos y mañana se borran solos. Cuando uno se acabe,
        toca <strong>Disponible</strong> para marcarlo <strong>Agotado</strong>.
      </p>

      {error && <p className="mesas-error">{error}</p>}

      <div className="carta-tarjeta">
        {cargando ? (
          <p className="mesas-vacio">Cargando...</p>
        ) : (
          <>
            {platos.length === 0 && <p className="mesas-vacio carta-vacia">Aún no hay platos. Escribe el primero aquí abajo.</p>}
            <ul className="carta-lista">
              {platos.map((p) => (
                <FilaPlato
                  key={`${p.id}-${p.nombre}-${p.precio}`}
                  plato={p}
                  ocupado={ocupado}
                  onGuardar={(id, cambios) => ejecutar(() => api.cartaActualizar(id, cambios))}
                  onAgotado={(id, agotado) => ejecutar(() => api.cartaActualizar(id, { agotado }))}
                  onQuitar={(id) => ejecutar(() => api.cartaQuitar(id))}
                />
              ))}
            </ul>
          </>
        )}

        <form className="carta-fila carta-nueva" onSubmit={agregar}>
          <input
            ref={nombreNuevoRef}
            className="mesas-input carta-nombre"
            placeholder="Nuevo plato (ej. Ají de gallina)"
            value={nuevoNombre}
            maxLength={80}
            enterKeyHint="next"
            onChange={(e) => setNuevoNombre(e.target.value)}
            aria-label="Nombre del plato nuevo"
          />
          <label className="carta-precio">
            <span>S/</span>
            <input
              className="mesas-input"
              inputMode="decimal"
              value={nuevoPrecio}
              onChange={(e) => setNuevoPrecio(e.target.value)}
              onFocus={(e) => e.target.select()}
              aria-label="Precio del plato nuevo"
            />
          </label>
          <button type="submit" className="mesas-boton-primario carta-agregar" disabled={ocupado}>
            <Plus size={16} /> Agregar
          </button>
        </form>
      </div>
    </div>
  );
}
