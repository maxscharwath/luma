import React from 'react';

/** Poster tile with generated key-art background, genre overline, title, badge and optional progress. */
export function PosterCard({
  title,
  genre,
  badge = '4K',
  colors = ['#3A2E5C', '#0E1430'],
  progress = null,
  width = 208,
  onClick,
}) {
  const poster = 'linear-gradient(158deg, ' + colors[0] + ' 0%, ' + colors[1] + ' 70%)';
  return React.createElement(
    'div',
    { onClick, style: { width, cursor: onClick ? 'pointer' : 'default' } },
    React.createElement(
      'div',
      {
        style: {
          position: 'relative',
          aspectRatio: '2 / 3',
          borderRadius: 'var(--radius-lg)',
          overflow: 'hidden',
          background: poster,
          boxShadow: 'var(--shadow-card)',
        },
      },
      React.createElement('div', {
        style: {
          position: 'absolute',
          inset: 0,
          background: 'linear-gradient(170deg,rgba(0,0,0,.05) 35%,rgba(0,0,0,.72))',
        },
      }),
      badge &&
        React.createElement(
          'div',
          {
            style: {
              position: 'absolute',
              top: 10,
              right: 10,
              font: '700 10px var(--font-ui)',
              padding: '4px 7px',
              borderRadius: 5,
              background: 'rgba(10,10,12,.6)',
              color: 'var(--luma-accent)',
            },
          },
          badge,
        ),
      React.createElement(
        'div',
        { style: { position: 'absolute', left: 14, right: 14, bottom: 15 } },
        genre &&
          React.createElement(
            'div',
            {
              style: {
                font: '700 10px var(--font-ui)',
                letterSpacing: '.12em',
                textTransform: 'uppercase',
                color: 'rgba(255,255,255,.6)',
                marginBottom: 5,
              },
            },
            genre,
          ),
        React.createElement(
          'div',
          { style: { font: '700 20px var(--font-display)', color: '#fff' } },
          title,
        ),
      ),
      progress != null &&
        React.createElement(
          'div',
          {
            style: {
              position: 'absolute',
              left: 0,
              right: 0,
              bottom: 0,
              height: 5,
              background: 'rgba(255,255,255,.22)',
            },
          },
          React.createElement('div', {
            style: { height: '100%', width: progress + '%', background: 'var(--luma-accent)' },
          }),
        ),
    ),
  );
}
