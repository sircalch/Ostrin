(function () {
  const SITE_FACTS = Object.freeze(globalThis.OSTRIN_SITE_FACTS || {});

  const released = SITE_FACTS.releaseStatus === 'published';
  document.querySelectorAll('.version').forEach(function (label) {
    label.textContent = released ? 'v' + SITE_FACTS.version + ' / experimental' : 'development / ' + SITE_FACTS.version;
  });

  // The release line only claims what scripts/site-facts.mjs derived from CHANGELOG.md.
  document.querySelectorAll('[data-release-line]').forEach(function (line) {
    const text = line.querySelector('[data-release-text]');
    if (!text) return;
    line.dataset.state = released ? 'published' : 'unreleased';
    if (released) {
      text.textContent = 'Ostrin v' + SITE_FACTS.version + ' · experimental developer release · published ' + SITE_FACTS.releaseDate + ' · ';
      const link = document.createElement('a');
      link.href = SITE_FACTS.releaseUrl;
      link.textContent = 'release notes ↗';
      text.append(link);
    } else {
      text.textContent = 'Version ' + SITE_FACTS.version + ' is in development; no release has been published yet.';
    }
  });

  document.querySelectorAll('[data-site-value]').forEach(function (node) {
    const value = SITE_FACTS[node.dataset.siteValue];
    if (value !== undefined) node.textContent = value;
  });

  document.querySelectorAll('.site-footer > span').forEach(function (label) {
    label.textContent = 'Open development · MIT License';
  });

  const header = document.querySelector('.site-header');
  const menuButton = document.querySelector('.menu-toggle');
  const navigation = header?.querySelector('.site-nav');

  if (header && menuButton && navigation) {
    if (!navigation.id) navigation.id = 'site-navigation';
    menuButton.setAttribute('aria-controls', navigation.id);

    function setMenu(active, restoreFocus) {
      header.classList.toggle('menu-active', active);
      document.body.classList.toggle('menu-open', active);
      menuButton.setAttribute('aria-expanded', String(active));
      menuButton.setAttribute('aria-label', active ? 'Close navigation' : 'Open navigation');
      if (active && restoreFocus) navigation.querySelector('a')?.focus();
      if (!active && restoreFocus) menuButton.focus();
    }

    menuButton.addEventListener('click', function () {
      setMenu(!header.classList.contains('menu-active'), true);
    });

    document.addEventListener('keydown', function (event) {
      if (event.key === 'Escape' && header.classList.contains('menu-active')) {
        event.preventDefault();
        setMenu(false, true);
      }
    });

    setMenu(false, false);
  }

  document.querySelectorAll('.site-nav a').forEach(function (link) {
    link.addEventListener('click', function () {
      if (header && menuButton && navigation) {
        header.classList.contains('menu-active') && menuButton.focus();
        header.classList.remove('menu-active');
        document.body.classList.remove('menu-open');
        menuButton.setAttribute('aria-expanded', 'false');
        menuButton.setAttribute('aria-label', 'Open navigation');
      }
    });
  });

  const filterButtons = document.querySelectorAll('.filter-button');
  filterButtons.forEach(function (button) {
    button.setAttribute('aria-pressed', String(button.classList.contains('active')));
    button.addEventListener('click', function () {
      const filter = button.dataset.filter;
      filterButtons.forEach(function (item) {
        item.classList.toggle('active', item === button);
        item.setAttribute('aria-pressed', String(item === button));
      });
      document.querySelectorAll('.example-card').forEach(function (card) {
        card.classList.toggle('is-hidden', filter !== 'all' && card.dataset.category !== filter);
      });
    });
  });

  const tabs = [...document.querySelectorAll('.tab-button')];
  function selectTab(button, moveFocus) {
    const target = button.dataset.tab;
    tabs.forEach(function (item) {
      const active = item === button;
      item.classList.toggle('active', active);
      item.setAttribute('aria-selected', String(active));
      item.tabIndex = active ? 0 : -1;
    });
    document.querySelectorAll('.tab-panel').forEach(function (panel) {
      const active = panel.id === target;
      panel.classList.toggle('active', active);
      panel.hidden = !active;
      if (active) panel.setAttribute('aria-labelledby', button.id);
    });
    if (moveFocus) button.focus();
  }

  tabs.forEach(function (button, index) {
    const target = button.dataset.tab;
    if (!button.id) button.id = `tab-${target}`;
    button.setAttribute('aria-controls', target);
    button.setAttribute('aria-selected', String(button.classList.contains('active')));
    button.tabIndex = button.classList.contains('active') ? 0 : -1;
    const panel = document.getElementById(target);
    if (panel) {
      panel.setAttribute('aria-labelledby', button.id);
      panel.hidden = !button.classList.contains('active');
    }
    button.addEventListener('click', function () {
      selectTab(button, false);
    });
    button.addEventListener('keydown', function (event) {
      if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key) || tabs.length < 2) return;
      event.preventDefault();
      const next = event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? tabs.length - 1
          : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
      selectTab(tabs[next], true);
    });
  });

  document.querySelectorAll('.copy-button').forEach(function (button) {
    button.addEventListener('click', async function () {
      const target = document.getElementById(button.dataset.copy);
      if (!target || !navigator.clipboard) return;
      try {
        await navigator.clipboard.writeText(target.innerText);
        const label = button.textContent;
        button.textContent = 'copied';
        setTimeout(function () { button.textContent = label; }, 1200);
      } catch (_) {
        button.textContent = 'select code';
      }
    });
  });
})();
