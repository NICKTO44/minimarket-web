import { useEffect, useMemo, useState } from 'react';
import { Check } from 'lucide-react';
import { api } from '../../api/api';
import { GRUPOS_UNIDADES, UNIDAD_BASE, unidadesRecomendadas } from '../../utils/unidades';
import './ConfiguracionUnidades.css';

const TOTAL = GRUPOS_UNIDADES.reduce((n, g) => n + g.unidades.length, 0);

function mismas(a, b) {
  return a.size === b.size && [...a].every((x) => b.has(x));
}

/**
 * Configuración → Unidades: el administrador marca las unidades de venta
 * que usa su negocio. Solo esas aparecen al crear o editar un producto.
 * "Unidad" y las que ya usan productos no se pueden apagar.
 */
export default function ConfiguracionUnidades({ restaurante = false }) {
  const [activas, setActivas] = useState(() => new Set());
  const [guardadas, setGuardadas] = useState(() => new Set());
  // unidad -> cuántos productos la usan
  const [enUso, setEnUso] = useState(() => new Map());
  const [cargando, setCargando] = useState(true);
  const [guardando, setGuardando] = useState(false);
  const [mensaje, setMensaje] = useState(null);

  const recibir = (datos) => {
    setActivas(new Set(datos.activas));
    setGuardadas(new Set(datos.activas));
    setEnUso(new Map(datos.en_uso.map((e) => [e.unidad, e.productos])));
  };

  useEffect(() => {
    api
      .unidades()
      .then(recibir)
      .catch((e) => setMensaje({ tipo: 'error', texto: e.message }))
      .finally(() => setCargando(false));
  }, []);

  // En un restaurante sus unidades van primero; en una tienda, al final.
  const grupos = useMemo(
    () => [...GRUPOS_UNIDADES].sort((a, b) => Number(!!b.restaurante === restaurante) - Number(!!a.restaurante === restaurante)),
    [restaurante]
  );

  const fija = (valor) => valor === UNIDAD_BASE || enUso.has(valor);

  const alternar = (valor) => {
    if (fija(valor)) return;
    setMensaje(null);
    setActivas((actual) => {
      const nuevo = new Set(actual);
      if (nuevo.has(valor)) nuevo.delete(valor);
      else nuevo.add(valor);
      return nuevo;
    });
  };

  // Lo que nunca se apaga: la unidad base y las que ya usan productos.
  const conFijas = (lista) => new Set([UNIDAD_BASE, ...enUso.keys(), ...lista]);

  const guardar = async () => {
    setGuardando(true);
    try {
      recibir(await api.unidadesGuardar([...activas]));
      setMensaje({ tipo: 'exito', texto: 'Unidades guardadas. Ya son las que aparecen al crear un producto.' });
    } catch (e) {
      setMensaje({ tipo: 'error', texto: e.message });
    } finally {
      setGuardando(false);
    }
  };

  if (cargando) return <p className="cfg-cargando">Cargando...</p>;

  const sinCambios = mismas(activas, guardadas);

  return (
    <div className="cfg-card">
      <h3 className="cfg-subtitulo-seccion">Unidades de venta</h3>
      <p className="cfg-nota-moneda">
        Marca solo las unidades que usa tu negocio. Son las únicas que aparecerán al crear o editar un producto.
      </p>

      {mensaje && <p className={`cfg-mensaje cfg-mensaje-${mensaje.tipo}`}>{mensaje.texto}</p>}

      <div className="uni-acciones">
        <button
          type="button"
          className="uni-boton uni-boton-primario"
          onClick={() => setActivas(conFijas(unidadesRecomendadas(restaurante)))}
        >
          Usar las recomendadas para {restaurante ? 'Restaurante' : 'Tienda'}
        </button>
        <button
          type="button"
          className="uni-boton"
          onClick={() => setActivas(new Set(GRUPOS_UNIDADES.flatMap((g) => g.unidades.map((u) => u.valor))))}
        >
          Marcar todas
        </button>
        <span className="uni-contador">
          {activas.size} de {TOTAL} activas
        </span>
      </div>

      {grupos.map((grupo) => (
        <div className="uni-grupo" key={grupo.titulo}>
          <h4>{grupo.titulo}</h4>
          <div className="uni-chips">
            {grupo.unidades.map((u) => {
              const marcada = activas.has(u.valor);
              const bloqueada = fija(u.valor);
              const productos = enUso.get(u.valor);
              return (
                <button
                  type="button"
                  key={u.valor}
                  role="checkbox"
                  aria-checked={marcada}
                  aria-disabled={bloqueada || undefined}
                  className={`uni-chip${marcada ? ' marcada' : ''}${bloqueada ? ' fija' : ''}`}
                  onClick={() => alternar(u.valor)}
                  title={
                    productos
                      ? 'No se puede quitar: hay productos que la usan'
                      : u.valor === UNIDAD_BASE
                        ? 'Siempre activa'
                        : undefined
                  }
                >
                  <span className="uni-check">{marcada && <Check size={13} strokeWidth={3.5} />}</span>
                  <span className="uni-texto">
                    {u.label}
                    {productos ? (
                      <small>
                        En uso · {productos} {productos === 1 ? 'producto' : 'productos'}
                      </small>
                    ) : (
                      u.valor === UNIDAD_BASE && <small>Siempre activa</small>
                    )}
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      ))}

      <div className="uni-pie">
        <button type="button" className="cfg-boton-guardar" onClick={guardar} disabled={guardando || sinCambios}>
          {guardando ? 'Guardando...' : sinCambios ? 'Sin cambios' : 'Guardar unidades'}
        </button>
      </div>
    </div>
  );
}
